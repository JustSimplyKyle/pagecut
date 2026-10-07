{
  description = "app";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    dioxus.url = "github:DioxusLabs/dioxus/main";
    dioxus.inputs.nixpkgs.follows = "nixpkgs";
  };
  outputs = { self, nixpkgs, utils, rust-overlay, dioxus }:
    utils.lib.eachDefaultSystem (system:
      let
        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rustToolchain = pkgs.rust-bin.nightly.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "rustc-codegen-cranelift-preview" ];
        };
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };
        # Prefer the shared C++ runtime when linking with mold.
        clangMold = pkgs.writeShellScriptBin "clang-mold" ''
          exec ${pkgs.clang}/bin/clang \
            -L${pkgs.stdenv.cc.cc.lib}/lib \
            -fuse-ld=mold \
            "$@"
        '';
        # Include Cranelift's assembly objects in Dioxus hot patches.
        dioxusCli = dioxus.packages.${system}.dioxus-cli.overrideAttrs (old: {
          patches = (old.patches or [ ]) ++ [
            ./patches/dioxus-cli-cranelift-asm-objects.patch
          ];
        });
      in
      {
        packages.default = rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          version = cargoToml.package.version;
          src = ./.;
          cargoHash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
          nativeBuildInputs = [ pkgs.pkg-config pkgs.clang pkgs.mold clangMold ];
          buildInputs = [ pkgs.openssl ];
        };
        devShells.default = pkgs.mkShell rec {
          buildInputs = [
            rustToolchain
            pkgs.pkg-config
            pkgs.just
            pkgs.clang
            pkgs.mold
            clangMold
            dioxusCli
            pkgs.samply
            pkgs.wayland
            pkgs.libxkbcommon
            pkgs.vulkan-loader
            pkgs.libGL
            pkgs.dbus
            pkgs.zenity
          ];
          LD_LIBRARY_PATH = "$LD_LIBRARY_PATH:${builtins.toString (pkgs.lib.makeLibraryPath buildInputs)}";
          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
          # LIBCLANG_PATH="${pkgs.libclang.lib}/lib";

        };
      });
}
