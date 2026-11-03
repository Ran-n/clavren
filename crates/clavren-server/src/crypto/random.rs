// -----------------------------------------------------------------------------
//+ Authors: 	Ran# <ran.hash@proton.me>
//+ Created: 	2026/11/02 14:53:29.556364
//+ Revised: 	2026/11/03 04:54:19.630774
// -----------------------------------------------------------------------------

//! Randomness from the operating system's cryptographic generator.

#[derive(Debug, thiserror::Error)]
#[error("the operating system's random number generator failed: {0}")]
pub struct RandomError(String);

pub fn random_bytes<const LENGTH: usize>() -> Result<[u8; LENGTH], RandomError> {
    let mut bytes = [0u8; LENGTH];
    getrandom::fill(&mut bytes).map_err(|error| RandomError(error.to_string()))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_draws_differ() {
        assert_ne!(random_bytes::<32>().unwrap(), random_bytes::<32>().unwrap());
    }
}
