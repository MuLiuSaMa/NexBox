use aes::cipher::{generic_array::GenericArray, BlockEncrypt, KeyInit};
use aes::Aes128;
use base64::Engine;
use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use md5::{Digest, Md5};
use num_bigint::BigUint;

/// 网易云 EAPI 加密密钥
const NETEASE_EAPI_KEY: &[u8; 16] = b"e82ckenh8dichen8";

/// 计算 MD5 哈希
pub fn md5_bytes(input: &[u8]) -> [u8; 16] {
    let mut hasher = Md5::new();
    hasher.update(input);
    hasher.finalize().into()
}

/// AES-128-ECB 加密 (PKCS7 填充)
pub fn aes_ecb_encrypt_pkcs7(input: &[u8], key: &[u8; 16]) -> Vec<u8> {
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut buffer = input.to_vec();
    let pad_len = 16 - (buffer.len() % 16);
    buffer.extend(std::iter::repeat_n(pad_len as u8, pad_len));

    for chunk in buffer.as_chunks_mut::<16>().0 {
        cipher.encrypt_block(GenericArray::from_mut_slice(chunk));
    }

    buffer
}

/// 网易云 EAPI 加密
/// 1. 计算 digest = MD5("nobody{api_path}use{payload}md5forencrypt")
/// 2. 拼接 data = "{api_path}-36cd479b6b5-{payload}-36cd479b6b5-{digest}"
/// 3. AES-128-ECB 加密 data
/// 4. 十六进制大写编码
pub fn encrypt_eapi_payload(api_path: &str, payload_text: &str) -> Result<String, String> {
    let digest_source = format!("nobody{api_path}use{payload_text}md5forencrypt");
    let digest = hex::encode(md5_bytes(digest_source.as_bytes()));
    let data = format!("{api_path}-36cd479b6b5-{payload_text}-36cd479b6b5-{digest}");
    let encrypted = aes_ecb_encrypt_pkcs7(data.as_bytes(), NETEASE_EAPI_KEY);
    Ok(hex::encode_upper(encrypted))
}

/// 构建 EAPI 请求头 (作为 Cookie 发送)
pub fn build_eapi_header() -> serde_json::Map<String, serde_json::Value> {
    use serde_json::json;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let request_id = format!("{now_ms}_{:04}", rand::random::<u32>() % 1000);

    let mut map = serde_json::Map::new();
    map.insert("__csrf".into(), json!(""));
    map.insert("appver".into(), json!("8.0.0"));
    map.insert("buildver".into(), json!(now_ms / 1000));
    map.insert("channel".into(), json!(""));
    map.insert("deviceId".into(), json!(""));
    map.insert("mobilename".into(), json!(""));
    map.insert("resolution".into(), json!("1920x1080"));
    map.insert("os".into(), json!("android"));
    map.insert("osver".into(), json!(""));
    map.insert("requestId".into(), json!(request_id));
    map.insert("versioncode".into(), json!("140"));
    map.insert("MUSIC_U".into(), json!(""));
    map
}

// ============================================================
//  网易云 weapi 加密 (NeteaseCloudMusicApi 标准算法)
// ============================================================

/// weapi 预设密钥与 IV
const NETEASE_WEAPI_PRESET_KEY: &[u8; 16] = b"0CoJUm6Qyw8W8jud";
const NETEASE_WEAPI_IV: &[u8; 16] = b"0102030405060708";
/// weapi 公钥模数 (1024 位, 官方 128 字节, 无前导零)
const NETEASE_WEAPI_MODULUS: &str = "e0b509f6259df8642dbc35662901477df22677ec152b5ff68ace615bb7b725152b3ab17a876aea8a5aa76d2e417629ec4ee341f56135fccf695280104e0312ecbda92557c93870114af6c9d05c4f7f0c3685b7a46bee255932575cce10b424d813cfe4875d3e82047b97ddef52741d546b8e289dc6935b3ece0462db0a22b8e7";
/// 公钥指数
const NETEASE_WEAPI_E: u32 = 65537;

/// AES-128-CBC 加密 (PKCS7 填充)
fn aes_cbc_encrypt_pkcs7(input: &[u8], key: &[u8; 16], iv: &[u8; 16]) -> Vec<u8> {
    type Aes128CbcEnc = cbc::Encryptor<Aes128>;
    let enc = Aes128CbcEnc::new_from_slices(key, iv).expect("invalid AES key/iv length");
    enc.encrypt_padded_vec_mut::<Pkcs7>(input)
}

/// 裸 RSA 加密 (无填充): m = bytes(secret), c = m^e mod n, 输出 128 字节 hex
/// 对应 node-forge 的 encrypt(str, 'NONE') 行为
fn weapi_rsa_encrypt(secret_reversed: &[u8]) -> String {
    let n = BigUint::parse_bytes(NETEASE_WEAPI_MODULUS.as_bytes(), 16)
        .expect("invalid weapi modulus");
    let e = BigUint::from(NETEASE_WEAPI_E);
    let m = BigUint::from_bytes_be(secret_reversed);
    let c = m.modpow(&e, &n);
    let bytes = c.to_bytes_be();
    let mut out = vec![0u8; 128usize.saturating_sub(bytes.len())];
    out.extend_from_slice(&bytes);
    hex::encode(out)
}

/// 生成 weapi 随机 16 位 secretKey (base62)
fn weapi_random_secret_key() -> String {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    (0..16)
        .map(|_| CHARS[rand::random::<usize>() % 62] as char)
        .collect()
}

/// 网易云 weapi 加密
/// 1. 第一层 AES-CBC(presetKey) 加密 payload
/// 2. 第二层 AES-CBC(随机 secretKey) 加密第一层结果的 base64 字符串
/// 3. encSecKey = 裸 RSA 加密 secretKey 逆序
///
/// 返回 (params, encSecKey)
pub fn encrypt_weapi_payload(payload_text: &str) -> (String, String) {
    let secret_key = weapi_random_secret_key();
    let inner = aes_cbc_encrypt_pkcs7(
        payload_text.as_bytes(),
        NETEASE_WEAPI_PRESET_KEY,
        NETEASE_WEAPI_IV,
    );
    let inner_b64 = base64::engine::general_purpose::STANDARD.encode(&inner);
    let params_bytes = aes_cbc_encrypt_pkcs7(
        inner_b64.as_bytes(),
        secret_key.as_bytes().try_into().expect("16-byte secret key"),
        NETEASE_WEAPI_IV,
    );
    let params = base64::engine::general_purpose::STANDARD.encode(&params_bytes);
    let reversed = secret_key.chars().rev().collect::<String>();
    let enc_sec_key = weapi_rsa_encrypt(reversed.as_bytes());
    (params, enc_sec_key)
}
