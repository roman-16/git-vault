{
  description = "Transparent encryption for git that collapses everything it protects into one opaque file";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-darwin"
        "x86_64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      version = self.shortRev or self.dirtyShortRev or "dev";
    in
    {
      packages = forAllSystems (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "git-vault";
          inherit version;

          cargoLock.lockFile = ./Cargo.lock;
          src = self;

          nativeBuildInputs = [ pkgs.installShellFiles ];

          nativeCheckInputs = [ pkgs.git ];

          postInstall = ''
            $out/bin/git-vault man "$TMPDIR/man"
            for page in "$TMPDIR/man"/*.1; do
              installManPage "$page"
            done

            installShellCompletion --cmd git-vault \
              --bash <($out/bin/git-vault completions bash) \
              --fish <($out/bin/git-vault completions fish) \
              --zsh <($out/bin/git-vault completions zsh)
          '';

          meta = {
            description = "Transparent encryption for git that collapses everything it protects into one opaque file";
            homepage = "https://github.com/roman-16/git-vault";
            license = pkgs.lib.licenses.mit;
            mainProgram = "git-vault";
            platforms = pkgs.lib.platforms.linux ++ pkgs.lib.platforms.darwin;
          };
        };
      });

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.default}/bin/git-vault";
          meta.description = "Transparent encryption for git that hides names, paths and structure";
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
