//! Atomic encrypted persistence, recovery copies, and conflict detection.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use sha2::{Digest, Sha256};

use crate::{
    crypto::{MAX_VAULT_BYTES, VaultCipher},
    model::{VaultData, now},
};

/// The live, editable vault. A prepared save may run while `data` is edited.
/// Callers must run at most one save job per store, complete its receipt before
/// preparing another, and drain pending jobs before `save`, `import`, or `export`.
pub struct Store {
    pub data: VaultData,
    path: PathBuf,
    cipher: VaultCipher,
    fingerprint: [u8; 32],
}

/// An owned snapshot that can be encrypted and committed on a worker thread.
/// Dropping it also drops its wiping `VaultData` and zeroizing derived key.
pub struct SaveJob {
    data: VaultData,
    path: PathBuf,
    cipher: VaultCipher,
    fingerprint: [u8; 32],
}

/// Successful disk commit metadata, containing no plaintext notes or passwords.
#[derive(Debug)]
pub struct SaveReceipt {
    path: PathBuf,
    base_fingerprint: [u8; 32],
    saved_fingerprint: [u8; 32],
    modified_at: String,
}

impl SaveJob {
    /// Commit this snapshot with the same recovery and disk-conflict protection
    /// as a synchronous save. The original store is untouched on either outcome.
    pub fn run(mut self) -> Result<SaveReceipt> {
        let _lock = writer_lock(&self.path)?;
        let previous = read_unchanged(&self.path, self.fingerprint)?;
        self.data.meta.modified_at = now();
        let encrypted = self.cipher.encrypt(&self.data)?;
        atomic_write(&sibling_path(&self.path, ".bak"), &previous, true)
            .context("Could not write recovery backup; original vault was preserved")?;
        // Also catch a non-cooperating program that changed the original
        // while the backup was being flushed.
        read_unchanged(&self.path, self.fingerprint)?;
        atomic_write(&self.path, &encrypted, true).context("Could not save vault")?;
        Ok(SaveReceipt {
            path: self.path,
            base_fingerprint: self.fingerprint,
            saved_fingerprint: fingerprint(&encrypted),
            modified_at: self.data.meta.modified_at.clone(),
        })
    }
}

impl Store {
    pub fn open(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let path = resolve_path(path.as_ref(), false)?;
        let _lock = writer_lock(&path)?;
        let bytes = read_bounded(&path)?;
        let (cipher, data) = VaultCipher::open(password, &bytes)?;
        Ok(Self {
            data,
            path,
            cipher,
            fingerprint: fingerprint(&bytes),
        })
    }

    /// Creation uses a no-clobber commit, including if another process creates
    /// the destination between the initial check and the final write.
    pub fn create(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let path = resolve_path(path.as_ref(), true)?;
        let _lock = writer_lock(&path)?;
        if path.try_exists()? {
            bail!("A vault already exists here; unlock it instead");
        }
        let data = VaultData::new();
        let cipher = VaultCipher::new(password)?;
        let bytes = cipher.encrypt(&data)?;
        atomic_write(&path, &bytes, false).context("Could not create vault")?;
        Ok(Self {
            data,
            path,
            cipher,
            fingerprint: fingerprint(&bytes),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn backup_path(&self) -> PathBuf {
        sibling_path(&self.path, ".bak")
    }

    /// Take a snapshot without disk I/O, encryption, or key derivation. The
    /// caller may keep editing `data` while running this job on another thread.
    /// Apply its receipt before preparing another save for this store.
    pub fn prepare_save(&self) -> SaveJob {
        SaveJob {
            data: self.data.clone(),
            path: self.path.clone(),
            cipher: self.cipher.clone(),
            fingerprint: self.fingerprint,
        }
    }

    /// Advance the disk baseline after a successful job. Only save metadata is
    /// updated; edits made since the snapshot are never replaced by old data.
    pub fn complete_save(&mut self, receipt: SaveReceipt) -> Result<()> {
        if self.path != receipt.path || self.fingerprint != receipt.base_fingerprint {
            bail!(
                "Save belongs to an older or different vault session; reopen the vault before saving again"
            );
        }
        self.fingerprint = receipt.saved_fingerprint;
        self.data.meta.modified_at = receipt.modified_at;
        Ok(())
    }

    /// Synchronously save after draining pending jobs. Does not re-run Argon2
    /// or retain the master password. Each write uses a fresh OS-random nonce.
    pub fn save(&mut self) -> Result<()> {
        let receipt = self.prepare_save().run()?;
        self.complete_save(receipt)
    }

    /// Export current edits as an independently encrypted snapshot, even when
    /// a disk conflict prevents saving the active vault.
    pub fn export(&mut self, destination: impl AsRef<Path>) -> Result<()> {
        let destination = resolve_path(destination.as_ref(), true)?;
        if self.reserved_path(&destination) {
            bail!("Choose an export destination outside the active vault and its recovery files");
        }
        let bytes = self.cipher.encrypt(&self.data)?;
        atomic_write(&destination, &bytes, true).context("Could not export encrypted vault")
    }

    /// Authenticate and validate the incoming vault before touching this one.
    /// The previous encrypted vault remains available at `backup_path()` and
    /// a permanent `vault.json.pre-import-<uuid>.bak` recovery copy.
    pub fn import(&mut self, source: impl AsRef<Path>, password: &str) -> Result<()> {
        let bytes = read_bounded(source.as_ref()).context("Could not read imported vault")?;
        let (cipher, data) = VaultCipher::open(password, &bytes)?;
        let _lock = writer_lock(&self.path)?;
        let previous = self.read_unchanged()?;
        // Unlike the rolling .bak file, this recovery copy survives subsequent
        // autosaves of the imported vault and keeps the original master key.
        let import_backup = sibling_path(
            &self.path,
            &format!(".pre-import-{}.bak", uuid::Uuid::new_v4().simple()),
        );
        atomic_write(&import_backup, &previous, false)
            .context("Could not preserve a permanent pre-import backup; import was cancelled")?;
        atomic_write(&self.backup_path(), &previous, true)
            .context("Could not back up the current vault; import was cancelled")?;
        self.read_unchanged()?;
        atomic_write(&self.path, &bytes, true).context("Could not install imported vault")?;
        self.data = data;
        self.cipher = cipher;
        self.fingerprint = fingerprint(&bytes);
        Ok(())
    }

    fn read_unchanged(&self) -> Result<Vec<u8>> {
        read_unchanged(&self.path, self.fingerprint)
    }

    fn reserved_path(&self, path: &Path) -> bool {
        let permanent_backup = same_path(
            path.parent().unwrap_or(Path::new("")),
            self.path.parent().unwrap_or(Path::new("")),
        ) && path.file_name().is_some_and(|name| {
            name.to_string_lossy().to_lowercase().starts_with(&format!(
                "{}.pre-import-",
                self.path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase()
            ))
        });
        permanent_backup
            || [
                self.path.clone(),
                self.backup_path(),
                sibling_path(&self.path, ".write-lock"),
                sibling_path(&self.path, ".lock"),
            ]
            .iter()
            .any(|candidate| same_path(candidate, path))
    }
}

fn read_unchanged(path: &Path, expected_fingerprint: [u8; 32]) -> Result<Vec<u8>> {
    let bytes = read_bounded(path)
        .context("Vault was removed or cannot be read; changes were not saved")?;
    if fingerprint(&bytes) != expected_fingerprint {
        bail!(
            "Vault changed on disk since it was unlocked. Changes were not saved; reopen the vault before editing again"
        );
    }
    Ok(bytes)
}

fn same_path(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

fn fingerprint(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn sibling_path(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn resolve_path(path: &Path, create_parent: bool) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = absolute
        .parent()
        .context("Vault path has no parent directory")?;
    if create_parent {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(parent)
            .context("Could not create vault directory")?;
    }
    let resolved = parent.canonicalize()?.join(
        absolute
            .file_name()
            .context("Vault path has no file name")?,
    );
    reject_symlink(&resolved)?;
    Ok(resolved)
}

fn reject_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            bail!("Refusing to replace a symbolic link: {}", path.display())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let metadata =
        fs::metadata(path).with_context(|| format!("Could not read {}", path.display()))?;
    if !metadata.is_file() {
        bail!("Vault path must be a regular file");
    }
    if metadata.len() > MAX_VAULT_BYTES as u64 {
        bail!("Vault exceeds the 32 MiB file size limit");
    }
    let file = File::open(path)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_VAULT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_VAULT_BYTES {
        bail!("Vault exceeds the 32 MiB file size limit");
    }
    Ok(bytes)
}

fn writer_lock(path: &Path) -> Result<File> {
    let lock_path = sibling_path(path, ".write-lock");
    reject_symlink(&lock_path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(lock_path)
        .context("Could not open the vault write lock")?;
    file.try_lock_exclusive()
        .context("Another process is writing this vault; try again shortly")?;
    Ok(file)
}

fn atomic_write(path: &Path, bytes: &[u8], replace: bool) -> Result<()> {
    reject_symlink(path)?;
    let parent = path.parent().context("File has no parent directory")?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".secure-notepad-")
        .tempfile_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    let file = if replace {
        temporary.persist(path)?
    } else {
        temporary.persist_noclobber(path)?
    };
    file.sync_all()?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Note;

    const PASSWORD: &str = "storage test passphrase";

    fn create() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::create(dir.path().join("vault.json"), PASSWORD).unwrap();
        (dir, store)
    }

    #[test]
    fn saves_and_keeps_previous_encrypted_version() {
        let (_dir, mut store) = create();
        let original = fs::read(store.path()).unwrap();
        store.data.notes.items.push(Note {
            content: "never plaintext on disk".into(),
            ..Default::default()
        });
        store.save().unwrap();
        assert_eq!(fs::read(store.backup_path()).unwrap(), original);
        let current = fs::read(store.path()).unwrap();
        assert!(!String::from_utf8_lossy(&current).contains("never plaintext on disk"));
        let reopened = Store::open(store.path(), PASSWORD).unwrap();
        assert_eq!(
            reopened.data.notes.items[0].content,
            "never plaintext on disk"
        );
        assert!(
            Store::open(store.backup_path(), PASSWORD)
                .unwrap()
                .data
                .notes
                .items
                .is_empty()
        );
    }

    #[test]
    fn background_save_preserves_newer_edits_and_next_save_catches_up() {
        let (_dir, mut store) = create();
        store.data.notes.items.push(Note::new("first snapshot"));
        let original_fingerprint = store.fingerprint;
        let job = store.prepare_save();
        let (resume, paused) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            paused.recv().unwrap();
            job.run()
        });

        // The live editor remains editable while its snapshot is in flight.
        store.data.notes.items[0].title = "newer edit".into();
        store.data.notes.items.push(Note::new("added while saving"));
        resume.send(()).unwrap();
        let receipt = worker.join().unwrap().unwrap();
        assert_eq!(store.fingerprint, original_fingerprint);
        let snapshot = Store::open(store.path(), PASSWORD).unwrap();
        assert_eq!(snapshot.data.notes.items.len(), 1);
        assert_eq!(snapshot.data.notes.items[0].title, "first snapshot");

        store.complete_save(receipt).unwrap();
        assert_eq!(store.data.notes.items[0].title, "newer edit");
        assert_eq!(store.data.notes.items[1].title, "added while saving");
        assert_eq!(store.fingerprint, snapshot.fingerprint);
        assert_eq!(store.data.meta.modified_at, snapshot.data.meta.modified_at);

        let receipt = std::thread::spawn({
            let job = store.prepare_save();
            move || job.run()
        })
        .join()
        .unwrap()
        .unwrap();
        store.complete_save(receipt).unwrap();
        let reopened = Store::open(store.path(), PASSWORD).unwrap();
        assert_eq!(reopened.data.notes.items[0].title, "newer edit");
        assert_eq!(reopened.data.notes.items[1].title, "added while saving");
        let backup = Store::open(store.backup_path(), PASSWORD).unwrap();
        assert_eq!(backup.data.notes.items[0].title, "first snapshot");
    }

    #[test]
    fn background_disk_conflict_preserves_edits_and_save_metadata() {
        let (_dir, mut store) = create();
        let mut concurrent = Store::open(store.path(), PASSWORD).unwrap();
        store.data.notes.items.push(Note::new("pending snapshot"));
        let original_fingerprint = store.fingerprint;
        let original_timestamp = store.data.meta.modified_at.clone();
        let job = store.prepare_save();
        store.data.notes.items[0].title = "newer pending edit".into();
        concurrent.data.notes.items.push(Note::new("external edit"));
        concurrent.save().unwrap();
        let current = fs::read(store.path()).unwrap();
        let backup = fs::read(store.backup_path()).unwrap();

        assert!(
            job.run()
                .unwrap_err()
                .to_string()
                .contains("changed on disk")
        );
        assert_eq!(fs::read(store.path()).unwrap(), current);
        assert_eq!(fs::read(store.backup_path()).unwrap(), backup);
        assert_eq!(store.fingerprint, original_fingerprint);
        assert_eq!(store.data.meta.modified_at, original_timestamp);
        assert_eq!(store.data.notes.items[0].title, "newer pending edit");
    }

    #[test]
    fn stale_snapshot_cannot_overwrite_a_completed_save() {
        let (_dir, mut store) = create();
        store.data.notes.items.push(Note::new("stale snapshot"));
        let stale = store.prepare_save();
        store.data.notes.items[0].title = "committed snapshot".into();
        store
            .complete_save(store.prepare_save().run().unwrap())
            .unwrap();
        let committed = fs::read(store.path()).unwrap();
        assert!(stale.run().is_err());
        assert_eq!(fs::read(store.path()).unwrap(), committed);
        // The failed stale job did not invalidate the current store's baseline.
        store.save().unwrap();
    }

    #[test]
    fn receipt_cannot_change_a_different_vault_with_the_same_baseline() {
        let (dir, store) = create();
        let other_path = dir.path().join("other.json");
        fs::copy(store.path(), &other_path).unwrap();
        let mut other = Store::open(&other_path, PASSWORD).unwrap();
        let original_fingerprint = other.fingerprint;
        let original_timestamp = other.data.meta.modified_at.clone();
        let receipt = store.prepare_save().run().unwrap();
        assert!(other.complete_save(receipt).is_err());
        assert_eq!(other.fingerprint, original_fingerprint);
        assert_eq!(other.data.meta.modified_at, original_timestamp);
        other.save().unwrap();
    }

    #[test]
    fn stale_receipt_cannot_change_a_reopened_vault_baseline() {
        let (_dir, store) = create();
        let receipt = store.prepare_save().run().unwrap();
        let mut reopened = Store::open(store.path(), PASSWORD).unwrap();
        let original_fingerprint = reopened.fingerprint;
        let original_timestamp = reopened.data.meta.modified_at.clone();
        assert!(reopened.complete_save(receipt).is_err());
        assert_eq!(reopened.fingerprint, original_fingerprint);
        assert_eq!(reopened.data.meta.modified_at, original_timestamp);
        reopened.save().unwrap();
    }

    #[test]
    fn dropping_an_owned_snapshot_leaves_live_data_and_key_usable() {
        let (_dir, mut store) = create();
        store.data.notes.items.push(Note::new("still present"));
        let before = fs::read(store.path()).unwrap();
        let mut job = store.prepare_save();
        job.data.notes.items[0].title = "discarded snapshot".into();
        // SaveJob owns VaultData (whose Drop wipes every plaintext field) and
        // VaultCipher (whose key is Zeroizing), rather than borrowed UI state.
        drop(job);
        assert_eq!(fs::read(store.path()).unwrap(), before);
        assert_eq!(store.data.notes.items[0].title, "still present");
        store.save().unwrap();
        assert_eq!(
            Store::open(store.path(), PASSWORD)
                .unwrap()
                .data
                .notes
                .items[0]
                .title,
            "still present"
        );
    }

    #[test]
    fn creation_never_overwrites_existing_vault() {
        let (_dir, store) = create();
        let before = fs::read(store.path()).unwrap();
        assert!(Store::create(store.path(), "different password").is_err());
        assert_eq!(fs::read(store.path()).unwrap(), before);
    }

    #[test]
    fn stale_session_and_deleted_vault_do_not_overwrite() {
        let (_dir, mut first) = create();
        let mut second = Store::open(first.path(), PASSWORD).unwrap();
        first.data.notes.items.push(Note::new("first"));
        first.save().unwrap();
        let before = fs::read(first.path()).unwrap();
        second.data.notes.items.push(Note::new("second"));
        assert!(
            second
                .save()
                .unwrap_err()
                .to_string()
                .contains("changed on disk")
        );
        assert_eq!(fs::read(first.path()).unwrap(), before);
        fs::remove_file(first.path()).unwrap();
        assert!(first.save().is_err());
        assert!(!first.path().exists());
    }

    #[test]
    fn backup_failure_preserves_vault_and_pending_edits() {
        let (_dir, mut store) = create();
        let before = fs::read(store.path()).unwrap();
        let old_timestamp = store.data.meta.modified_at.clone();
        fs::create_dir(store.backup_path()).unwrap();
        store.data.notes.items.push(Note::new("pending"));
        assert!(store.save().is_err());
        assert_eq!(fs::read(store.path()).unwrap(), before);
        assert_eq!(store.data.notes.items[0].title, "pending");
        assert_eq!(store.data.meta.modified_at, old_timestamp);
    }

    #[test]
    fn import_authenticates_before_replacing_and_preserves_backup() {
        let (dir, mut store) = create();
        let original = fs::read(store.path()).unwrap();
        let source = dir.path().join("import.json");
        fs::write(&source, include_bytes!("../tests/fixtures/python-v1.json")).unwrap();
        assert!(store.import(&source, "incorrect").is_err());
        assert_eq!(fs::read(store.path()).unwrap(), original);
        assert!(!store.backup_path().exists());
        store.import(&source, "Compatibility test 🔐").unwrap();
        assert_eq!(fs::read(store.backup_path()).unwrap(), original);
        assert_eq!(store.data.passwords[0].password, "pāsswörd-日本語");
        assert!(Store::open(store.path(), PASSWORD).is_err());
        store.save().unwrap();
        assert!(Store::open(store.path(), "Compatibility test 🔐").is_ok());
        let permanent_backups: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains(".pre-import-")
            })
            .collect();
        assert_eq!(permanent_backups.len(), 1);
        assert_eq!(fs::read(&permanent_backups[0]).unwrap(), original);
        assert!(Store::open(&permanent_backups[0], PASSWORD).is_ok());
    }

    #[test]
    fn export_is_encrypted_and_cannot_overwrite_recovery_files() {
        let (dir, mut store) = create();
        let export = dir.path().join("export.json");
        store.data.notes.items.push(Note::new("exported"));
        store.export(&export).unwrap();
        assert_eq!(
            Store::open(&export, PASSWORD).unwrap().data.notes.items[0].title,
            "exported"
        );
        let backup = store.backup_path();
        assert!(store.export(backup).is_err());
        let own_path = store.path().to_owned();
        assert!(store.export(own_path).is_err());
    }

    #[test]
    fn conflict_recovery_exports_unsaved_edits_without_touching_original() {
        let (dir, mut store) = create();
        let mut concurrent = Store::open(store.path(), PASSWORD).unwrap();
        concurrent.data.notes.items.push(Note::new("on disk"));
        concurrent.save().unwrap();
        let before = fs::read(store.path()).unwrap();
        let old_backup = fs::read(store.backup_path()).unwrap();
        store.data.notes.items.push(Note::new("unsaved edits"));
        assert!(store.save().is_err());
        let destination = dir.path().join("recovered.json");
        store.export(&destination).unwrap();
        assert_eq!(
            Store::open(destination, PASSWORD).unwrap().data.notes.items[0].title,
            "unsaved edits"
        );
        assert_eq!(fs::read(store.path()).unwrap(), before);
        assert_eq!(fs::read(store.backup_path()).unwrap(), old_backup);
    }

    #[test]
    fn write_lock_contention_and_oversized_input_are_rejected() {
        let (dir, mut store) = create();
        let lock = writer_lock(store.path()).unwrap();
        assert!(store.save().is_err());
        drop(lock);
        let oversized = dir.path().join("oversized.json");
        File::create(&oversized)
            .unwrap()
            .set_len(MAX_VAULT_BYTES as u64 + 1)
            .unwrap();
        assert!(Store::open(oversized, PASSWORD).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn vault_and_backup_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, mut store) = create();
        store.save().unwrap();
        for path in [store.path().to_owned(), store.backup_path()] {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}
