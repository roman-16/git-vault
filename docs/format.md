# The `.vault` format

Version 1. This page is normative: the format is meant to outlive this implementation, so anything written here can be relied on, and anything not written here cannot.

## Layout

```text
prefix
  magic          8 bytes    "GITVAULT"
  version        1 byte     0x01
  key id         8 bytes    identifies the key without revealing it

entries, sorted by id, each independent
  id            16 bytes    BLAKE3 keyed hash of the path
  nonce         24 bytes    BLAKE3 keyed hash of the sealed record
  length         4 bytes    little endian, the length of `sealed`
  sealed         length     XChaCha20-Poly1305 of the record below
```

Entries run to the end of the file; there is no count. Trailing bytes are an error, and so is an id that is not greater than the one before it.

## The sealed record

Each entry seals one record, which is what padding and encryption are applied to:

```text
kind             1 byte     1 = regular file, 2 = symlink
executable       1 byte     0 or 1
path length      2 bytes    little endian
content length   4 bytes    little endian
path             variable   repository-relative, "/" separated, UTF-8
content          variable   the file, or the link's target
padding          variable   zero bytes, up to the size class
```

Because the record is padded before it is sealed, the padding is inside the authenticated ciphertext and is invisible from outside.

## Size classes

A record is padded to the next power of two, with a floor of 256 bytes. A sealed entry therefore reveals its secret's size only to within a factor of two, and every small secret looks identical in size to every other small secret.

## Keys

Everything is derived from one 32-byte vault key, using BLAKE3's key derivation with a distinct context string per purpose, so no key is ever used for two jobs:

| Derived key | Context | Used for |
| --- | --- | --- |
| id key | `git-vault 2026-01 entry id` | keyed hash of a path, truncated to 16 bytes |
| nonce key | `git-vault 2026-01 entry nonce` | keyed hash of a record, truncated to 24 bytes |
| content key | `git-vault 2026-01 entry content` | the XChaCha20-Poly1305 key |
| key id | `git-vault 2026-01 key id` | the 8 bytes in the prefix |

## Determinism

The same secrets under the same key always produce byte-identical output. This is a hard requirement rather than a nicety: the vault is sealed again on every git command, and any drift would leave `.vault/data` permanently modified.

Three things make it hold. Entries are sorted by id, so the order they were collected in cannot leak or change the bytes. Nonces are derived from the records they protect rather than generated, which makes this a deterministic authenticated encryption scheme. And nothing anywhere is timestamped.

The consequence to be aware of is that identical plaintext produces identical ciphertext, so somebody watching the repository can tell that a secret went back to a value it had before. → [Threat model](threat-model.md)

## Reading a vault by hand

The prefix is plaintext, so anybody can check what they are looking at, count the entries and see their sizes, with no key:

```console
$ head --bytes=9 .vault/data | xxd
00000000: 4749 5456 4155 4c54 01                   GITVAULT.
```

`git vault ls` needs a key, and so does anything that reveals a path.

## Compatibility

The version byte is checked, and a vault from a future version is refused with a message saying to update, rather than being guessed at. Within version 1, every field above is fixed.

`.vault/keys` is not part of this format. It is a stock age file, so `age --decrypt --identity ~/.config/git-vault/identity .vault/keys` returns the raw 32-byte vault key without this tool being involved.
