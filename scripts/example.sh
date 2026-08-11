#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
root=$PWD
binary=$root/${GIT_VAULT:-target/release/git-vault}
example=$root/example

if [ ! -x "$binary" ]; then
    printf 'No binary at %s. Run `just build` first.\n' "$binary" >&2
    exit 1
fi

rm --recursive --force "$example"
mkdir --parents "$example/bin" "$example/identities"
ln --symbolic "$binary" "$example/bin/git-vault"

export PATH="$example/bin:$PATH"
export GIT_CONFIG_GLOBAL=/dev/null
export GIT_CONFIG_SYSTEM=/dev/null
export GIT_TERMINAL_PROMPT=0
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_PREFIX

become() {
    export GIT_VAULT_IDENTITY="$example/identities/$1"
    export GIT_VAULT_LABEL="$1@$2"
    export GIT_AUTHOR_NAME="$1" GIT_AUTHOR_EMAIL="$1@example.com"
    export GIT_COMMITTER_NAME="$1" GIT_COMMITTER_EMAIL="$1@example.com"
}

public_key_of() {
    sed --quiet 's/^# public key: //p' "$example/identities/$1"
}

git init --bare --quiet --initial-branch main "$example/origin.git"

become you laptop
git init --quiet --initial-branch main "$example/you"
cd "$example/you"
git remote add origin ../origin.git
git-vault init >/dev/null
git-vault add secrets/ config/ci.key >/dev/null
mkdir --parents secrets config src
printf '# my-project\n\nAn ordinary project that keeps its secrets in git.\n' > README.md
printf 'fn main() {\n    println!("hello");\n}\n' > src/main.rs
printf 'STRIPE_KEY=sk_live_51HxYqL\nDATABASE_URL=postgres://prod\n' > secrets/prod.env
printf 'deploy-token-9f2b1c\n' > secrets/deploy.token
printf 'ci-signing-key-4a7e\n' > config/ci.key
git add --all
git commit --quiet --message 'seal the secrets'
git push --quiet --set-upstream origin main

for persona in mate stranger; do
    become "$persona" desktop
    git clone --quiet "$example/origin.git" "$example/$persona"
    cd "$example/$persona"
    git remote set-url origin ../origin.git
    git-vault unlock >/dev/null 2>&1 || true
done

become you laptop
cd "$example/you"
git-vault share "$(public_key_of mate)" --label mate@desktop >/dev/null
git add .vault/keys .vault/recipients
git commit --quiet --message 'share the vault with mate'
git push --quiet

become mate desktop
cd "$example/mate"
git pull --quiet
git-vault unlock >/dev/null

cat > "$example/activate" <<ACTIVATE
export GIT_VAULT_EXAMPLE=$example
ACTIVATE
cat >> "$example/activate" <<'ACTIVATE'

PATH="$GIT_VAULT_EXAMPLE/bin:$PATH"
export PATH
export GIT_CONFIG_GLOBAL=/dev/null
export GIT_CONFIG_SYSTEM=/dev/null
export GIT_TERMINAL_PROMPT=0
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_PREFIX

git-vault-persona() {
    export GIT_VAULT_IDENTITY="$GIT_VAULT_EXAMPLE/identities/$1"
    export GIT_VAULT_LABEL="$1@$2"
    export GIT_AUTHOR_NAME="$1" GIT_AUTHOR_EMAIL="$1@example.com"
    export GIT_COMMITTER_NAME="$1" GIT_COMMITTER_EMAIL="$1@example.com"
    cd "$GIT_VAULT_EXAMPLE/$1"
    printf 'You are %s in %s\n' "$1" "$PWD"
}

you() { git-vault-persona you laptop; }
mate() { git-vault-persona mate desktop; }
stranger() { git-vault-persona stranger desktop; }

you
ACTIVATE

printf 'The playground is at %s\n\n' "$example"
printf '  source example/activate    a scrubbed shell with the release binary on PATH\n'
printf '  you                       the owner, unlocked, secrets on disk\n'
printf '  mate                      a second recipient, unlocked\n'
printf '  stranger                  a clone with no access\n\n'
printf 'They share example/origin.git, so push and pull work between them.\n'
printf 'Run `just example` again for a clean one; it deletes whatever is there.\n'
