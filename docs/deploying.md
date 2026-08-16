# Deploying secrets

`git vault unlock` sets up a clone you work in. `git vault unseal` is the other half: it takes a sealed file and a key, and writes the secrets into a directory. No repository, no git, no wiring.

```console
$ git vault unseal --data ./vault/data --keys ./vault/keys \
                   --identity /etc/ssh/ssh_host_ed25519_key \
                   --into /run/git-vault
Unsealed 3 secrets into /run/git-vault.
```

It prints a count and not the names, because the names are the thing this tool exists to hide and a server's output ends up in a log. `--verbose` lists them when you want that.

## The rule that decides everything

**Anything that reads your repository through git cannot see your secrets.** They are untracked and ignored, which is exactly how their names stay hidden. So a Nix flake, `cargo package`, `npm pack`, a Docker build context fed by `git archive`, Bazel and a sparse CI checkout all see the sealed file and nothing else. → [Limitations](limitations.md)

That leaves one shape that works, and it is the same shape sops-nix and agenix use:

> Seal only what your build does not read. Unseal the rest at deploy time.

If a build step has to read a secret, no transparent-encryption tool can help; that secret belongs in a private repository instead.

## In CI

A runner has no identity, so give it the vault key.

```bash
git vault export-key ci.key            # once, from a machine that has access
```

```yaml
- run: |
    printf '%s' "$VAULT_KEY_BASE64" | base64 --decode > /tmp/vault.key
    git vault unseal --data .vault/data --key-file /tmp/vault.key --into ./secrets
```

Inside a checkout you can equally use `git vault unlock --key-file`, which puts the secrets back where they belong in the worktree. Reach for `unseal` when the job has no checkout, or when the secrets should land somewhere else.

## On a server

Give the machine access with its own SSH host key. Nothing secret moves: `.pub` files are public.

```console
$ scp root@homelab:/etc/ssh/ssh_host_ed25519_key.pub homelab.pub
$ git vault share homelab.pub --label homelab
ssh-ed25519 …0cu6dRLf11 can now open the vault.

$ git add .vault && git commit -m 'give homelab access'
```

Then, on the machine, whenever the secrets are needed:

```bash
git vault unseal --data /srv/config/.vault/data \
                 --keys /srv/config/.vault/keys \
                 --identity /etc/ssh/ssh_host_ed25519_key \
                 --mode 0400 --into /run/git-vault
```

The directory is created `0700` when git-vault creates it, and `--mode` gives every file it writes the mode you name. A passphrase-protected SSH key is refused rather than silently failing to match, because age cannot use `ssh-agent`. → [Keys and access](keys-and-access.md)

## On NixOS

`.vault/data` and `.vault/keys` are tracked, so a flake copies them into the store for free. Ciphertext in the store is harmless. Unseal at activation, never at build time: a derivation that unsealed would need the key inside the sandbox, which puts the key in the store.

```nix
systemd.services.git-vault-secrets = {
  before = [ "cloudflared.service" ];
  requiredBy = [ "cloudflared.service" ];
  serviceConfig = {
    ExecStart = ''
      ${pkgs.git-vault}/bin/git-vault unseal \
        --data ${./.vault/data} \
        --keys ${./.vault/keys} \
        --identity /etc/ssh/ssh_host_ed25519_key \
        --entries 'hosts/homelab/**' \
        --mode 0400 \
        --into /run/git-vault
    '';
    RemainAfterExit = true;
    Type = "oneshot";
  };
};
```

Services then read the files, rather than having values baked into them:

```nix
serviceConfig.EnvironmentFile = "/run/git-vault/hosts/homelab/cloudflared.env";
```

Two things to be clear about before adopting this.

**A value read at evaluation time cannot come from a vault.** `builtins.readFile ./secrets.json` runs while Nix builds your configuration, and at that moment the file does not exist as far as Nix is concerned. Every such use has to become a file read at runtime first. That is a change to your configuration, not to git-vault, and it is worth doing on its own: a value interpolated into a unit file or a `writeText` lands in the world-readable store, which is the leak this replaces.

**One vault key opens the whole vault.** `--entries` controls what a host is *given*, not what it *could* read: any host that can unseal can unseal everything. If your hosts do not trust each other, give them separate vaults in separate repositories. → [Threat model](threat-model.md)
