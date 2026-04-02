# DocVault — Project Manifest

> Persistent project identity document. Created by `/adopt` on 2026-04-02. Updated by `/close-cycle` at the end of each cycle.

---

## Project Identity

| Field | Value |
|-------|-------|
| **Project Name** | DocVault |
| **Repository** | AnshulJethva10/DocVault |
| **Type** | web-app |
| **Governance Mode** | standard |
| **License** | MIT |
| **Adopted** | 2026-04-02 |
| **Framework Version** | antigravity-ultimate-sdlc v1.0 |

## Description

DocVault is a 100% client-side web application that disguises sensitive files by encrypting them with AES-256-GCM and encoding the ciphertext into PNG images or AVI videos. It enables safe storage on cloud services that only accept image or video formats. All cryptographic operations execute in the browser via a Rust-compiled WebAssembly module — files never leave the user's device.

## Tech Stack

| Layer | Technology | Version |
|-------|-----------|---------|
| Crypto Engine | Rust + `ring` | 1.70+ |
| WASM Bridge | `wasm-pack` + `wasm-bindgen` | 0.2.x |
| Frontend | React + TypeScript | 18.3 / 5.7 |
| Build | Vite | 5.4 |
| Styling | Tailwind CSS | 3.4 |
| AVI Muxer | Pure TypeScript (custom) | — |
| Package Manager | npm | 9+ |

## Feature Inventory (Cumulative)

| Feature ID | Name | Cycle Introduced | Status |
|-----------|------|-----------------|--------|
| F-001 | File Encryption & PNG Encoding | cycle-001-pre-existing | Shipped |
| F-002 | File Decryption & Restore | cycle-001-pre-existing | Shipped |
| F-003 | Multi-frame AVI Output (Large Files) | cycle-001-pre-existing | Shipped |
| F-004 | Batch Encoding | cycle-001-pre-existing | Shipped |
| F-005 | WASM/Browser Bridge & UX Shell | cycle-001-pre-existing | Shipped |
| F-006 | Native CLI Tool (`docvault` binary) | cycle-002-cli-tool | Shipped |
| F-007 | docvault-core Dual-Target (WASM + Native) | cycle-002-cli-tool | Shipped |

## Architecture Decisions

| ADR | Title | Status |
|-----|-------|--------|
| ADR-001 | Rust + WebAssembly Cryptography Core | Accepted |
| ADR-002 | 100% Client-Side Architecture (Zero Server) | Accepted |
| ADR-003 | Fixed 1024×1024 Frame Size + RGB Pixel Packing | Accepted |
| ADR-004 | React + Vite + TypeScript Frontend Stack | Accepted |
| ADR-005 | Pure TypeScript AVI Muxer (MPNG Codec) | Accepted |

## Key Constants (Permanent Reference)

| Constant | Value | Notes |
|----------|-------|-------|
| Frame size | 1024×1024 px | Fixed — changing breaks existing vaults |
| Bytes per frame | 3,145,728 | RGB only, no alpha channel |
| Metadata size | 276 bytes | Embedded in leading pixels of frame 1 |
| PBKDF2 iterations | 310,000 | Meets OWASP 2023 recommendation |
| GCM tag size | 16 bytes | Appended to ciphertext, before packing |
| Salt size | 32 bytes | Random, per-file, stored in metadata |
| IV size | 12 bytes | Random, per-file, stored in metadata |

## Cycle History

| Cycle | Type | Status | Summary |
|-------|------|--------|---------|
| cycle-001-pre-existing | Pre-existing | Archived | Full application built before framework adoption |
| cycle-002-cli-tool | Feature | **Completed** 2026-04-02 | Native Rust CLI binary; `docvault-core` dual-target refactor; 21 tests added |

## Roadmap (from README)

| Item | Status |
|------|--------|
| AES-256-GCM encoding to PNG | ✅ Done |
| PBKDF2 key derivation (310K) | ✅ Done |
| Fixed 1024×1024 multi-frame encoding | ✅ Done |
| AVI video output (MPNG codec) | ✅ Done |
| React + WASM browser app | ✅ Done |
| Batch encoding | ✅ Done |
| CLI tool (native Rust binary) | ✅ **Done — cycle-002-cli-tool** |
| Filename obfuscation option | ⬜ Planned |
| Desktop app (Tauri wrapper) | ⬜ Planned |

## Known Issues / Risks

| Issue | Severity | Notes |
|-------|---------|-------|
| npm audit: 3 vulnerabilities (1 HIGH, 2 MODERATE) | Medium | `picomatch` ReDoS + `esbuild`/`vite` dev-server CORS — dev-only tools, not in production bundle |
| No CI/CD pipeline | Low | Manual build process only |
| Large-file memory pressure | Medium | Browser tab memory limit for very large files across many frames |
| CLI spinner is synchronous (PBKDF2 blocks tick) | Low | Cosmetic; would require threading to fix |
| No shell completions for `docvault` binary | Low | clap can generate them; not yet wired up |
| Pre-existing unused import in `crypto.rs` | Low | `use ring::aead::{self, ...}` — one-line fix |
