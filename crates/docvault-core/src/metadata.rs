use serde::{Deserialize, Serialize};

/// Total metadata size in bytes: 4 + 1 + 8 + 32 + 12 + 128 + 64 + 27 = 276
pub const METADATA_SIZE: usize = 276;
/// Number of pixels needed to store metadata (276 bytes / 3 bytes per pixel = 92 pixels)
pub const METADATA_PIXELS: usize = 92;
/// Magic bytes identifying a DocVault encoded image
pub const MAGIC: [u8; 4] = *b"DCVT";
/// Current format version
pub const VERSION: u8 = 1;

#[derive(Debug, Clone)]
pub struct VaultMetadata {
    pub magic: [u8; 4],
    pub version: u8,
    pub file_size: u64,
    pub salt: [u8; 32],
    pub iv: [u8; 12],
    pub filename: [u8; 128],
    pub mime_type: [u8; 64],
    pub reserved: [u8; 27],
}

impl VaultMetadata {
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
            version: VERSION,
            file_size,
            salt,
            iv,
            filename: fname_buf,
            mime_type: mime_buf,
            reserved: [0u8; 27],
        }
    }

    /// Serialize metadata to exactly 276 bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(METADATA_SIZE);
        buf.extend_from_slice(&self.magic);          // 4 bytes
        buf.push(self.version);                       // 1 byte
        buf.extend_from_slice(&self.file_size.to_le_bytes()); // 8 bytes
        buf.extend_from_slice(&self.salt);            // 32 bytes
        buf.extend_from_slice(&self.iv);              // 12 bytes
        buf.extend_from_slice(&self.filename);        // 128 bytes
        buf.extend_from_slice(&self.mime_type);       // 64 bytes
        buf.extend_from_slice(&self.reserved);        // 27 bytes
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

        let mut reserved = [0u8; 27];
        reserved.copy_from_slice(&data[offset..offset + 27]);

        Ok(VaultMetadata {
            magic,
            version,
            file_size,
            salt,
            iv,
            filename,
            mime_type,
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

    /// Extract MIME type as a trimmed UTF-8 string (null bytes removed)
    pub fn mime_type_str(&self) -> String {
        let end = self.mime_type.iter().position(|&b| b == 0).unwrap_or(64);
        String::from_utf8_lossy(&self.mime_type[..end]).to_string()
    }
}
