//! Implements "crypto" used by savefiles
//! (it's just a bunch of XORs, actually)

use anyhow::{bail, Result};

use super::crc32::crc32;

fn chunks_transform(data: &mut [u8], key: u32, transform: impl Fn(&mut [u8; 4], &mut u32)) {
    let mut current_key = key;

    // I REALLY want array_chunks to be stable...
    for chunk_data in data.chunks_mut(4) {
        let chunk_data_len = chunk_data.len();
        let mut chunk = [0; 4];

        chunk[..chunk_data_len].copy_from_slice(chunk_data);
        transform(&mut chunk, &mut current_key);
        chunk_data.copy_from_slice(&chunk[..chunk_data_len]);
    }
}

fn decode_once(data: &mut [u8], key: u32) {
    #[inline]
    fn transform(data: &mut [u8; 4], key: &mut u32) {
        let word = u32::from_be_bytes(*data);
        // transform the data
        *data = (word ^ *key).to_be_bytes();

        // transform the key. Use the encrypted CRC!
        *key ^= super::crc32::crc32(&word.to_le_bytes(), 0)
    }

    chunks_transform(data, key, transform);
}

fn encode_once(data: &mut [u8], key: u32) {
    #[inline]
    fn transform(data: &mut [u8; 4], key: &mut u32) {
        let word = u32::from_be_bytes(*data);
        // transform the data
        *data = (word ^ *key).to_be_bytes();

        // transform the key. Use the encrypted CRC!
        *key ^= crc32(&(word ^ *key).to_le_bytes(), 0)
    }

    chunks_transform(data, key, transform);
}

pub fn decode(data: &[u8], key: u32) -> Result<Vec<u8>> {
    let mut stage1 = data.to_vec();
    decode_once(&mut stage1, key);
    let stage2_len = stage1.len() - 4;
    let (stage2, crc) = stage1.split_at_mut(stage2_len);
    let crc = u32::from_le_bytes(crc.try_into().unwrap());
    decode_once(stage2, crc); // yes, they use CRC as a key...
    if crc32(stage2, 0) != crc {
        bail!("save obfuscation CRC mismatch")
    }

    stage1.drain(stage2_len..);

    Ok(stage1)
}

pub fn encode(data: &[u8], key: u32) -> Vec<u8> {
    let mut data = data.to_vec();
    let crc = crc32(&data, 0);
    encode_once(&mut data, crc);
    data.extend(&crc.to_le_bytes());

    encode_once(&mut data, key);

    data
}

#[cfg(test)]
mod test {
    use insta::assert_debug_snapshot;
    use rand::{Rng, RngCore, SeedableRng};

    fn decode_once(data: &str, key: u32) -> String {
        let mut data = hex::decode(data).unwrap();
        super::decode_once(&mut data, key);
        hex::encode(data)
    }

    #[test]
    fn test_decode_once_simpl() {
        assert_debug_snapshot!(decode_once("0123456789abcdef", 0x1337), @r###""01235650b1e1c6a2""###);
    }

    #[test]
    fn test_decode_once_random() {
        // generate random test cases
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x42);

        for _ in 0..100 {
            let len = rng.gen_range(1..20);
            let data = (0..len).map(|_| rng.gen::<u8>()).collect::<Vec<_>>();
            let data = hex::encode(data);

            let key = rng.next_u32();
            assert_debug_snapshot!(
                format!("decode_once_random_{}", data),
                decode_once(&data, key)
            );
        }
    }

    fn assert_encode_decode_once(data: &[u8], key: u32) {
        let orig_data = hex::encode(data);

        let mut data = data.to_vec();
        super::encode_once(&mut data, key);
        super::decode_once(&mut data, key);

        let data = hex::encode(data);
        assert_eq!(data, orig_data);
    }

    #[test]
    fn test_encode_decode_once_random() {
        // generate random test cases
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x42);

        for _ in 0..100 {
            let len = rng.gen_range(1..20);
            let data = (0..len).map(|_| rng.gen::<u8>()).collect::<Vec<_>>();

            assert_encode_decode_once(&data, rng.next_u32());
        }
    }

    #[test]
    fn test_encode_decode_random() {
        // generate random test cases
        let mut rng = rand::rngs::StdRng::seed_from_u64(0x42);

        for _ in 0..100 {
            let len = rng.gen_range(1..20);
            let data = (0..len).map(|_| rng.gen::<u8>()).collect::<Vec<_>>();

            let key = rng.next_u32();
            let encoded = super::encode(&data, key);
            let decoded = super::decode(&encoded, key).unwrap();
            assert_eq!(decoded, data);
        }
    }
}
