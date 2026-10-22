/// Total metadata size in bytes: 4 + 1 + 8 + 32 + 12 + 128 + 64 + 2 + 25 = 276
pub const METADATA_SIZE: usize = 276;
/// Number of pixels needed to store metadata (276 bytes / 3 bytes per pixel = 92 pixels)
pub const METADATA_PIXELS: usize = 92;
/// Magic bytes identifying a DocVault encoded image
pub const MAGIC: [u8; 4] = *b"DCVT";
/// Current format version.
/// v1: metadata header is NOT authenticated.
/// v2: metadata header is passed to AES-GCM as associated data, so any
///     tampering with filename, size, salt, etc. fails decryption.
/// v3: filename and MIME type move inside the ciphertext (the outer header
///     no longer reveals them), and the PBKDF2 iteration count is stored in
///     `reserved[0..4]` so it can be raised without another format bump.
pub const VERSION: u8 = 3;
/// Last version that stores the filename and MIME type in the clear.
pub const LEGACY_VERSION: u8 = 2;
/// Oldest format version the decoder still accepts.
pub const MIN_SUPPORTED_VERSION: u8 = 1;
/// Fallback name used when the stored filename is empty or unsafe.
pub const FALLBACK_FILENAME: &str = "restored_file";
/// MIME type used when none is known or the given one is too long.
pub const DEFAULT_MIME: &str = "application/octet-stream";

/// PBKDF2 iterations for new vaults (OWASP 2023 guidance for PBKDF2-HMAC-SHA256).
pub const PBKDF2_ITERATIONS: u32 = 600_000;
/// Bounds accepted from a v3 header. The header is read before the password is
/// checked, so an attacker-chosen count must not stall the decoder for hours.
pub const MIN_PBKDF2_ITERATIONS: u32 = 100_000;
pub const MAX_PBKDF2_ITERATIONS: u32 = 10_000_000;

/// Longest filename stored in a v3 vault (the common filesystem limit).
pub const MAX_FILENAME_BYTES: usize = 255;
/// Longest MIME type stored in a v3 vault.
pub const MAX_MIME_BYTES: usize = 255;

#[derive(Debug, Clone)]
pub struct VaultMetadata {
    pub magic: [u8; 4],
    pub version: u8,
    /// Length of the encrypted payload, excluding the 16-byte GCM tag.
    /// v1/v2: the original file. v3: the inner header plus the original file.
    pub file_size: u64,
    pub salt: [u8; 32],
    pub iv: [u8; 12],
    /// v1/v2 only; all zeros in v3.
    pub filename: [u8; 128],
    /// v1/v2 only; all zeros in v3.
    pub mime_type: [u8; 64],
    /// Total number of frames (1 = single PNG, >1 = multi-frame video)
    pub total_frames: u16,
    pub reserved: [u8; 25],
}

impl VaultMetadata {
    /// Build a legacy (v2) header that stores the filename and MIME type in the clear.
    /// New vaults use [`VaultMetadata::new_v3`]; this remains for compatibility tests.
    pub fn new(filename: &str, mime_type: &str, file_size: u64, salt: [u8; 32], iv: [u8; 12]) -> Self {
        let mut fname_buf = [0u8; 128];
        let fname_bytes = filename.as_bytes();
        let fname_len = fname_bytes.len().min(128);
        fname_buf[..fname_len].copy_from_slice(&fname_bytes[..fname_len]);

        let mut mime_buf = [0u8; 64];
        let mime_bytes = mime_type.as_bytes();
        let mime_len = mime_bytes.len().min(64);
        mime_buf[..mime_len].copy_from_slice(&mime_bytes[..mime_len]);

        VaultMetadata {
            magic: MAGIC,
            version: LEGACY_VERSION,
            file_size,
            salt,
            iv,
            filename: fname_buf,
            mime_type: mime_buf,
            total_frames: 1,
            reserved: [0u8; 25],
        }
    }

    /// Build a v3 header. `payload_len` is the length of the inner header plus file.
    pub fn new_v3(payload_len: u64, salt: [u8; 32], iv: [u8; 12], total_frames: u16, iterations: u32) -> Self {
        let mut reserved = [0u8; 25];
        reserved[..4].copy_from_slice(&iterations.to_le_bytes());
        VaultMetadata {
            magic: MAGIC,
            version: VERSION,
            file_size: payload_len,
            salt,
            iv,
            filename: [0u8; 128],
            mime_type: [0u8; 64],
            total_frames,
            reserved,
        }
    }

    /// PBKDF2 iteration count used to derive this vault's key.
    pub fn iterations(&self) -> u32 {
        if self.version >= 3 {
            u32::from_le_bytes([self.reserved[0], self.reserved[1], self.reserved[2], self.reserved[3]])
        } else {
            crate::crypto::LEGACY_PBKDF2_ITERATIONS
        }
    }

    /// Serialize metadata to exactly 276 bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(METADATA_SIZE);
        buf.extend_from_slice(&self.magic);                        // 4 bytes
        buf.push(self.version);                                    // 1 byte
        buf.extend_from_slice(&self.file_size.to_le_bytes());      // 8 bytes
        buf.extend_from_slice(&self.salt);                         // 32 bytes
        buf.extend_from_slice(&self.iv);                           // 12 bytes
        buf.extend_from_slice(&self.filename);                     // 128 bytes
        buf.extend_from_slice(&self.mime_type);                    // 64 bytes
        buf.extend_from_slice(&self.total_frames.to_le_bytes());   // 2 bytes
        buf.extend_from_slice(&self.reserved);                     // 25 bytes
        debug_assert_eq!(buf.len(), METADATA_SIZE);
        buf
    }

    /// Deserialize metadata from a 276-byte slice
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < METADATA_SIZE {
            return Err(format!(
                "Metadata too short: expected {} bytes, got {}",
                METADATA_SIZE,
                data.len()
            ));
        }

        let mut offset = 0;

        let mut magic = [0u8; 4];
        magic.copy_from_slice(&data[offset..offset + 4]);
        offset += 4;

        let version = data[offset];
        offset += 1;

        let mut file_size_bytes = [0u8; 8];
        file_size_bytes.copy_from_slice(&data[offset..offset + 8]);
        let file_size = u64::from_le_bytes(file_size_bytes);
        offset += 8;

        let mut salt = [0u8; 32];
        salt.copy_from_slice(&data[offset..offset + 32]);
        offset += 32;

        let mut iv = [0u8; 12];
        iv.copy_from_slice(&data[offset..offset + 12]);
        offset += 12;

        let mut filename = [0u8; 128];
        filename.copy_from_slice(&data[offset..offset + 128]);
        offset += 128;

        let mut mime_type = [0u8; 64];
        mime_type.copy_from_slice(&data[offset..offset + 64]);
        offset += 64;

        // total_frames: 2 bytes (little-endian u16)
        let mut tf_bytes = [0u8; 2];
        tf_bytes.copy_from_slice(&data[offset..offset + 2]);
        let total_frames = u16::from_le_bytes(tf_bytes);
        offset += 2;

        let mut reserved = [0u8; 25];
        reserved.copy_from_slice(&data[offset..offset + 25]);

        // Backward compatibility: old images had 0 in the total_frames bytes
        // Treat 0 as 1 (single frame)
        let total_frames = if total_frames == 0 { 1 } else { total_frames };

        Ok(VaultMetadata {
            magic,
            version,
            file_size,
            salt,
            iv,
            filename,
            mime_type,
            total_frames,
            reserved,
        })
    }

    /// Validate that magic bytes are b"DCVT"
    pub fn validate_magic(&self) -> bool {
        self.magic == MAGIC
    }

    /// Extract filename as a trimmed UTF-8 string (null bytes removed)
    pub fn filename_str(&self) -> String {
        let end = self.filename.iter().position(|&b| b == 0).unwrap_or(128);
        String::from_utf8_lossy(&self.filename[..end]).to_string()
    }

    /// Filename reduced to a single safe path component.
    ///
    /// The stored filename comes from an untrusted file, so any directory
    /// parts (`../`, `/abs/`, `C:\\`) are dropped and characters that are
    /// invalid on common filesystems are replaced. Never returns a name that
    /// could escape the output directory.
    pub fn safe_filename(&self) -> String {
        sanitize_filename(&self.filename_str())
    }

    /// Extract MIME type as a trimmed UTF-8 string (null bytes removed)
    pub fn mime_type_str(&self) -> String {
        let end = self.mime_type.iter().position(|&b| b == 0).unwrap_or(64);
        String::from_utf8_lossy(&self.mime_type[..end]).to_string()
    }
}

// ─── v3 inner header ──────────────────────────────────────────────────────────
//
// Encrypted together with the file: [u16 LE name_len][name][u8 mime_len][mime]

/// Serialize the v3 inner header, truncating the name and MIME type to their limits.
pub fn inner_header(filename: &str, mime_type: &str) -> Vec<u8> {
    let name = truncate_filename(filename, MAX_FILENAME_BYTES);
    let mime = if mime_type.len() <= MAX_MIME_BYTES { mime_type } else { DEFAULT_MIME };
    let mut out = Vec::with_capacity(3 + name.len() + mime.len());
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    out.push(mime.len() as u8);
    out.extend_from_slice(mime.as_bytes());
    out
}

/// Parse a decrypted v3 payload into `(filename, mime_type, file_offset)`.
pub fn parse_inner_header(payload: &[u8]) -> Result<(String, String, usize), String> {
    const BAD: &str = "Corrupted vault: invalid inner header";
    let name_len = u16::from_le_bytes(payload.get(..2).ok_or(BAD)?.try_into().map_err(|_| BAD)?) as usize;
    if name_len > MAX_FILENAME_BYTES {
        return Err(BAD.to_string());
    }
    let name = payload.get(2..2 + name_len).ok_or(BAD)?;
    let mime_len = *payload.get(2 + name_len).ok_or(BAD)? as usize;
    let mime_start = 3 + name_len;
    let mime = payload.get(mime_start..mime_start + mime_len).ok_or(BAD)?;
    Ok((
        String::from_utf8_lossy(name).into_owned(),
        String::from_utf8_lossy(mime).into_owned(),
        mime_start + mime_len,
    ))
}

/// Shorten `name` to at most `max_bytes` UTF-8 bytes, keeping the extension
/// when there is one and never splitting a character.
pub fn truncate_filename(name: &str, max_bytes: usize) -> String {
    if name.len() <= max_bytes {
        return name.to_string();
    }
    let floor = |s: &str, n: usize| -> usize { (0..=n.min(s.len())).rev().find(|&i| s.is_char_boundary(i)).unwrap_or(0) };
    // Keep a short extension (".pdf", ".xlsx"); a very long "extension" is just part of the name
    match name.rfind('.') {
        Some(dot) if dot > 0 && name.len() - dot <= 16 && name.len() - dot < max_bytes => {
            let ext = &name[dot..];
            let stem_end = floor(name, max_bytes - ext.len());
            format!("{}{}", &name[..stem_end], ext)
        }
        _ => name[..floor(name, max_bytes)].to_string(),
    }
}

/// Device names Windows reserves regardless of extension (`NUL.txt` is still NUL).
const WINDOWS_RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Invisible formatting characters that can disguise a name, e.g. U+202E turns
/// "invoice\u{202E}fdp.exe" into something that displays as "invoiceexe.pdf".
fn is_format_char(c: char) -> bool {
    matches!(c,
        '\u{061C}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{2069}' | '\u{FEFF}')
}

/// Reduce an untrusted filename to one safe path component.
pub fn sanitize_filename(name: &str) -> String {
    // Keep only the last component, treating both separators as path breaks
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .filter(|&c| !is_format_char(c))
        .map(|c| match c {
            ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    // Windows silently drops trailing dots and spaces, so strip them up front
    let trimmed = cleaned.trim_end_matches(['.', ' ']).trim_start_matches(' ');
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return FALLBACK_FILENAME.to_string();
    }
    let stem = trimmed.split('.').next().unwrap_or("").trim_end();
    if WINDOWS_RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        format!("_{trimmed}")
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_plain_names() {
        assert_eq!(sanitize_filename("report.pdf"), "report.pdf");
        assert_eq!(sanitize_filename("my file (1).txt"), "my file (1).txt");
    }

    #[test]
    fn sanitize_strips_directories() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("/home/user/.bashrc"), ".bashrc");
        assert_eq!(sanitize_filename("..\\..\\Windows\\evil.dll"), "evil.dll");
        assert_eq!(sanitize_filename("C:\\evil.exe"), "evil.exe");
    }

    #[test]
    fn sanitize_rejects_empty_and_dot_names() {
        assert_eq!(sanitize_filename(""), FALLBACK_FILENAME);
        assert_eq!(sanitize_filename(".."), FALLBACK_FILENAME);
        assert_eq!(sanitize_filename("foo/.."), FALLBACK_FILENAME);
        assert_eq!(sanitize_filename("dir/"), FALLBACK_FILENAME);
        assert_eq!(sanitize_filename("..."), FALLBACK_FILENAME);
    }

    #[test]
    fn sanitize_replaces_reserved_chars() {
        assert_eq!(sanitize_filename("a:b*c?.txt"), "a_b_c_.txt");
        assert_eq!(sanitize_filename("new\nline.txt"), "new_line.txt");
    }

    #[test]
    fn sanitize_defuses_windows_device_names() {
        assert_eq!(sanitize_filename("CON"), "_CON");
        assert_eq!(sanitize_filename("nul.txt"), "_nul.txt");
        assert_eq!(sanitize_filename("com1.tar.gz"), "_com1.tar.gz");
        assert_eq!(sanitize_filename("console.txt"), "console.txt");
    }

    #[test]
    fn sanitize_removes_bidi_overrides() {
        assert_eq!(sanitize_filename("invoice\u{202E}fdp.exe"), "invoicefdp.exe");
        assert_eq!(sanitize_filename("\u{200B}\u{FEFF}"), FALLBACK_FILENAME);
    }

    #[test]
    fn truncate_keeps_extension_and_char_boundaries() {
        let long = format!("{}.pdf", "a".repeat(300));
        let t = truncate_filename(&long, 255);
        assert_eq!(t.len(), 255);
        assert!(t.ends_with(".pdf"));

        let deva = format!("{}.pdf", "दस्तावेज़".repeat(20));
        let t = truncate_filename(&deva, 255);
        assert!(t.len() <= 255 && t.ends_with(".pdf"));

        assert_eq!(truncate_filename("short.txt", 255), "short.txt");
    }

    #[test]
    fn inner_header_round_trips() {
        let docx = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
        let mut payload = inner_header("Quarterly report.docx", docx);
        payload.extend_from_slice(b"body");
        let (name, mime, off) = parse_inner_header(&payload).unwrap();
        assert_eq!(name, "Quarterly report.docx");
        assert_eq!(mime, docx);
        assert_eq!(&payload[off..], b"body");
    }

    #[test]
    fn inner_header_rejects_truncated_input() {
        assert!(parse_inner_header(&[]).is_err());
        assert!(parse_inner_header(&[10, 0, b'a']).is_err());
        assert!(parse_inner_header(&[0xFF, 0xFF]).is_err());
    }

    #[test]
    fn v3_header_stores_iterations_not_names() {
        let m = VaultMetadata::new_v3(42, [1; 32], [2; 12], 3, PBKDF2_ITERATIONS);
        let parsed = VaultMetadata::from_bytes(&m.to_bytes()).unwrap();
        assert_eq!(parsed.version, VERSION);
        assert_eq!(parsed.iterations(), PBKDF2_ITERATIONS);
        assert_eq!(parsed.total_frames, 3);
        assert!(parsed.filename.iter().all(|&b| b == 0));
        assert!(parsed.mime_type.iter().all(|&b| b == 0));
    }
}
