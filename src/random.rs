use std::thread;
use std::time::Duration;

use getrandom::getrandom;

pub fn fill_random(dest: &mut [u8]) {
    let mut retried = false;
    loop {
        if Ok(()) == getrandom(dest) {
            return;
        }

        if !retried {
            warn!("Failed to generate random sequence. Retrying...");
            retried = true;
        }

        thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_sequence() {
        let zeros = vec![0; 4096];
        let mut random = vec![0; 4096];
        fill_random(&mut random);
        assert_ne!(zeros, random);
    }
}
