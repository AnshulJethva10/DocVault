<p align="center">
  <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT">
  <img src="https://img.shields.io/badge/built%20with-Rust-orange.svg" alt="Built with Rust">
  <img src="https://img.shields.io/badge/architecture-100%25%20Client--Side-success.svg" alt="100% Client-Side">
  <img src="https://img.shields.io/badge/encryption-AES--256--GCM-red.svg" alt="AES-256-GCM">
</p>

# 🔐 DocVault

*Your files, disguised.*

**DocVault** disguises any document as a PNG image using **AES-256-GCM** encryption. It allows you to store sensitive documents in cloud services that only accept photos or videos. The cloud provider never sees your actual content. The application executes 100% client-side via **Rust** and **WebAssembly**. Your files never leave your device. The encoded image appears as visual noise and remains indecipherable without the exact password.

## Preview

<!-- Add a screenshot here -->
![DocVault Screenshot](screenshot.png)

The DocVault interface encoding a sensitive document into a secure, disguised PNG image.

## How It Works

```text
ENCODE                              DECODE
──────                              ──────
📄 document.pdf                     🖼️  document_vaulted.png
      │                                       │
      ▼                                       ▼
 AES-256-GCM                         Read RGB Pixels
 Encryption                                   │
      │                                       ▼
      ▼                                Extract Metadata
 Byte → RGB                           (salt, IV, filename)
 Pixel Mapping                                │
      │                                       ▼
      ▼                                AES-256-GCM
🖼️  document_vaulted.png              Decryption
  (looks like noise)                          │
                                              ▼
                                      📄 document.pdf ✓
```

Each 3 bytes of encrypted data map to one **RGB pixel**. The resulting PNG looks like abstract noise. DocVault embeds **metadata** including the `salt`, `IV`, and original `filename` in the first row of pixels.

## Features

| Feature | Description | Detail |
|---------|-------------|--------|
| **AES-256-GCM** | Authenticated encryption mode. | Guarantees confidentiality and prevents tampering. |
| **PBKDF2** | Secure key derivation. | Executes `310,000` iterations to thwart brute-force attacks. |
| **Zero Server** | 100% client-side processing. | Makes zero network requests and completely lacks telemetry. |
| **Rust Core** | High-performance WebAssembly engine. | Delivers native-like cryptographic speeds inside your browser. |
| **Adaptive Output** | PNG or Video output format. | Creates PNGs for files ≤ 10MB and WebM for files > 10MB. |
| **Lossless Recovery** | Exact byte-for-byte decryption. | Returns your original file with the exact original filename. |
| **Self-Contained** | Metadata travels with pixels. | Requires no external database or key files. |
| **Open Source** | Fully verifiable code. | Uses the MIT license for absolute transparency. |

## Security

### Encryption
DocVault encrypts your file using **AES-256-GCM**, an authenticated encryption mode. The authentication tag guarantees that nobody altered or corrupted the ciphertext. Providing a wrong password surfaces a clear authentication mismatch error. The engine never produces garbled output.

### Key Derivation
DocVault derives the encryption key using **PBKDF2-HMAC-SHA256** with `310,000` iterations. This high iteration count meets OWASP 2023 recommendations. It makes brute-force attacks computationally expensive. The engine generates a random 256-bit `salt` for every single file.

### What DocVault Does NOT Protect Against

⚠️ **Limitations:**
- Does not hide the fact that a PNG file exists.
- Security depends entirely on your password strength.
- The encoded image requires more storage than the original file.
- We have not formally audited this software.

## Getting Started

### Prerequisites
- Rust (1.70+): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- wasm-pack: `cargo install wasm-pack`
- Rust WASM target: `rustup target add wasm32-unknown-unknown`
- Node.js 18+ and npm 9+

### Installation & Build

```bash
# 1. Clone the repository
git clone https://github.com/yourusername/docvault.git
cd docvault

# 2. Build the Rust core → WebAssembly
cd crates/docvault-core
wasm-pack build --target web --out-dir pkg

# 3. Install frontend dependencies
cd ../../web
npm install

# 4. Start the development server
npm run dev
```

Then open `http://localhost:5173`.

### Production Build

```bash
cd web
npm run build
# Output → web/dist/ (deploy to any static host)
```

## Project Structure

```text
docvault/
├── crates/
│   └── docvault-core/        # Rust crate (compiles to WASM)
│       └── src/
│           ├── lib.rs        # wasm-bindgen exports
│           ├── crypto.rs     # AES-256-GCM + PBKDF2
│           ├── encoder.rs    # Byte → RGB pixel encoding
│           ├── decoder.rs    # RGB pixel → byte decoding
│           └── metadata.rs   # VaultMetadata struct (276 bytes)
└── web/                      # React + Vite frontend
    └── src/
        ├── hooks/
        │   └── useDocVault.ts    # WASM bridge hook
        └── components/           # UI components
```

## Usage

### Encoding a File
1. Open DocVault in your browser
2. Click **Encode** tab
3. Drag & drop or select any file (e.g., `quarterly_report.pdf`)
4. Enter a strong password
5. Click **Encode & Download**
6. Save `quarterly_report_vaulted.png` to your photo cloud storage

### Decoding a File
1. Open DocVault
2. Click **Decode** tab
3. Drop the `.png` file you previously encoded
4. Enter the same password
5. Click **Decode & Restore**
6. Your original file downloads automatically

> ⚠️ **Password Warning:** There is no password recovery mechanism. If you forget your password, the file cannot be recovered. Store your password safely.

> 📝 **Note:** The encoded PNG uses slightly more storage than the original file due to pixel padding and PNG metadata overhead.

## Built With

| Technology | Purpose | Version |
|------------|---------|---------|
| **Rust** | Core cryptography and encoding engine | 1.70+ |
| **wasm-pack** | WASM compilation and packaging | Latest |
| **wasm-bindgen** | JavaScript to Rust interop | Latest |
| **ring** | Cryptographic primitives (AES, PBKDF2) | Latest |
| **React** | Interactive user interface | 18.x |
| **TypeScript** | Type-safe frontend logic | 5.x |
| **Vite** | Frontend bundling and development server | 5.x |
| **Tailwind CSS** | Styling and layout system | 3.x |
| **serde-wasm-bindgen**| Data serialization across the WASM boundary | Latest |

## Roadmap

- [x] AES-256-GCM file encoding to PNG
- [x] PBKDF2 key derivation with 310K iterations
- [x] Video encoding for files > 10MB
- [x] React + WASM browser app
- [ ] CLI tool (native Rust binary, same core)
- [ ] Batch encoding (multiple files at once)
- [ ] Filename obfuscation option
- [ ] Drag-to-reorder encoded image collections
- [ ] Desktop app (Tauri wrapper)

## Contributing

We welcome issues and pull requests on the GitHub repository. This project uses Rust for the core and React for the UI. Familiarity with `wasm-bindgen` is extremely helpful for core contributions.

## License

Distributed under the MIT License. See `LICENSE` for more information.

## Disclaimer

DocVault represents a personal utility tool. Security professionals have not independently audited this software. Do not use it as your sole protection for critically sensitive data. The author holds no responsibility for any data loss resulting from forgotten passwords or software bugs.
