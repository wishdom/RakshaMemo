use anyhow::{Result, ensure};
use rand::{
    CryptoRng,
    rand_core::UnwrapErr,
    rngs::SysRng,
    seq::{IndexedRandom, SliceRandom},
};

const LOWERCASE: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const UPPERCASE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &[u8] = b"0123456789";
const SYMBOLS: &[u8] = b"!@#$%^&*()-_=+[]{};:,<.>/?";

#[derive(Clone, Debug)]
pub struct Options {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            length: 20,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
        }
    }
}

pub fn generate(options: &Options) -> Result<String> {
    // Like the previous OsRng, this panics only if the OS cannot supply randomness.
    generate_with_rng(options, &mut UnwrapErr(SysRng))
}

fn generate_with_rng(options: &Options, rng: &mut impl CryptoRng) -> Result<String> {
    ensure!(
        (8..=128).contains(&options.length),
        "Password length must be between 8 and 128 characters"
    );
    let pools: Vec<&[u8]> = [
        (options.lowercase, LOWERCASE),
        (options.uppercase, UPPERCASE),
        (options.digits, DIGITS),
        (options.symbols, SYMBOLS),
    ]
    .into_iter()
    .filter_map(|(selected, pool)| selected.then_some(pool))
    .collect();
    ensure!(!pools.is_empty(), "Select at least one character category");
    let alphabet: Vec<u8> = pools.iter().flat_map(|pool| pool.iter().copied()).collect();
    let mut password = Vec::with_capacity(options.length);
    // `choose` and `shuffle` use unbiased range sampling, unlike `% pool.len()`.
    for pool in &pools {
        password.push(*pool.choose(rng).expect("Character pools are nonempty"));
    }
    while password.len() < options.length {
        password.push(*alphabet.choose(rng).expect("The alphabet is nonempty"));
    }
    password.shuffle(rng);
    Ok(String::from_utf8(password).expect("The alphabet is ASCII"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::StdRng};

    #[test]
    fn all_category_combinations_obey_constraints_at_length_limits() {
        let mut rng = StdRng::seed_from_u64(1749);
        for selected in 1..16 {
            for length in [8, 20, 128] {
                let options = Options {
                    length,
                    lowercase: selected & 1 != 0,
                    uppercase: selected & 2 != 0,
                    digits: selected & 4 != 0,
                    symbols: selected & 8 != 0,
                };
                for _ in 0..40 {
                    let password = generate_with_rng(&options, &mut rng).unwrap();
                    assert_eq!(password.len(), length);
                    for (enabled, pool) in [
                        (options.lowercase, LOWERCASE),
                        (options.uppercase, UPPERCASE),
                        (options.digits, DIGITS),
                        (options.symbols, SYMBOLS),
                    ] {
                        assert_eq!(password.bytes().any(|byte| pool.contains(&byte)), enabled);
                    }
                }
            }
        }
    }

    #[test]
    fn invalid_options_return_errors() {
        for length in [0, 7, 129, usize::MAX] {
            assert!(
                generate(&Options {
                    length,
                    ..Options::default()
                })
                .is_err()
            );
        }
        assert!(
            generate(&Options {
                length: 20,
                lowercase: false,
                uppercase: false,
                digits: false,
                symbols: false,
            })
            .is_err()
        );
    }

    #[test]
    fn shuffle_does_not_leave_guaranteed_categories_at_fixed_positions() {
        let mut rng = StdRng::seed_from_u64(782);
        let mut first_categories = [false; 4];
        for _ in 0..100 {
            let password = generate_with_rng(&Options::default(), &mut rng).unwrap();
            for (index, pool) in [LOWERCASE, UPPERCASE, DIGITS, SYMBOLS].iter().enumerate() {
                first_categories[index] |= pool.contains(&password.as_bytes()[0]);
            }
        }
        assert!(first_categories.into_iter().all(|seen| seen));
    }
}
