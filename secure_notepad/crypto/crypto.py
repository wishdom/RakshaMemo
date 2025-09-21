from __future__ import annotations

import os
import json
import base64
from dataclasses import dataclass
from typing import Any, Dict

from argon2.low_level import hash_secret_raw, Type
from cryptography.hazmat.primitives.ciphers.aead import AESGCM


@dataclass
class KdfParams:
    time_cost: int = 3
    memory_cost_kib: int = 65536  # 64 MiB
    parallelism: int = 2
    key_len: int = 32

    def to_dict(self) -> Dict[str, Any]:
        return {
            "algo": "argon2id",
            "time": self.time_cost,
            "mem": self.memory_cost_kib,
            "parallel": self.parallelism,
            "length": self.key_len,
        }

    @staticmethod
    def from_dict(d: Dict[str, Any]) -> "KdfParams":
        return KdfParams(
            time_cost=int(d.get("time", 3)),
            memory_cost_kib=int(d.get("mem", 65536)),
            parallelism=int(d.get("parallel", 2)),
            key_len=int(d.get("length", 32)),
        )


def b64e(b: bytes) -> str:
    return base64.b64encode(b).decode("utf-8")


def b64d(s: str) -> bytes:
    return base64.b64decode(s.encode("utf-8"))


def derive_key(password: str, salt: bytes, params: KdfParams | None = None) -> bytes:
    if params is None:
        params = KdfParams()
    return hash_secret_raw(
        secret=password.encode("utf-8"),
        salt=salt,
        time_cost=params.time_cost,
        memory_cost=params.memory_cost_kib,
        parallelism=params.parallelism,
        hash_len=params.key_len,
        type=Type.ID,
    )


def encrypt_json(password: str, plaintext_obj: Dict[str, Any], *, params: KdfParams | None = None) -> bytes:
    params = params or KdfParams()
    salt = os.urandom(16)
    key = derive_key(password, salt, params)
    aes = AESGCM(key)
    nonce = os.urandom(12)
    plaintext = json.dumps(plaintext_obj, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    ct = aes.encrypt(nonce, plaintext, None)
    # AESGCM.encrypt returns ciphertext||tag; cryptography handles tag inside
    # But for clarity we keep it whole in ciphertext and store separately not needed. We'll store as single b64.
    container = {
        "v": 1,
        "kdf": params.to_dict(),
        "salt": b64e(salt),
        "cipher": {
            "alg": "aes-256-gcm",
            "nonce": b64e(nonce),
            "blob": b64e(ct),
        },
    }
    return json.dumps(container, separators=(",", ":")).encode("utf-8")


def decrypt_json(password: str, blob: bytes) -> Dict[str, Any]:
    container = json.loads(blob.decode("utf-8"))
    if container.get("v") != 1:
        raise ValueError("Unsupported file version")
    params = KdfParams.from_dict(container.get("kdf", {}))
    salt = b64d(container["salt"])
    key = derive_key(password, salt, params)
    nonce = b64d(container["cipher"]["nonce"])
    ct = b64d(container["cipher"]["blob"])
    aes = AESGCM(key)
    pt = aes.decrypt(nonce, ct, None)
    return json.loads(pt.decode("utf-8"))
