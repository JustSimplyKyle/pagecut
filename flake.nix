{
  description = "app";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    dioxus.url = "github:DioxusLabs/dioxus/main";
    dioxus.inputs.nixpkgs.follows = "nixpkgs";
    # Pin the sources used by Cargo's neighboring path dependencies.
    iced-source = {
      url = "github:JustSimplyKyle/iced/bd9d88e5a54d59acc74a49e9f1c4593dbcaab294";
      flake = false;
    };
    vse-source = {
      url = "github:JustSimplyKyle/videosubextract/504ce20035f559bf78b52556ea6e13738e3fa126";
      flake = false;
    };
    cosmic-source = {
      url = "github:pop-os/libcosmic/e9d8c30e9490c7badcddb7f2ab6a404e8ecccc4d";
      flake = false;
    };
    cosmic-iced-source = {
      url = "github:pop-os/iced/cac09daee4b49dbfa7a6661e85f36f3b2968ae74";
      flake = false;
    };
  };
  outputs =
    {
      self,
      nixpkgs,
      utils,
      rust-overlay,
      dioxus,
      iced-source,
      vse-source,
      cosmic-source,
      cosmic-iced-source,
    }:
    utils.lib.eachDefaultSystem (
      system:
      let
        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rustToolchain = pkgs.rust-bin.nightly.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
            "rustc-codegen-cranelift-preview"
          ];
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
        # Include Cranelift's assembly objects and support Cargo's new build layout.
        dioxusCli = dioxus.packages.${system}.dioxus-cli.overrideAttrs (old: {
          patches = (old.patches or [ ]) ++ [
            ./patches/dioxus-cli-cranelift-asm-objects.patch
            ./patches/dioxus-cli-cargo-build-layout.patch
          ];
        });
        # Recreate the path layout without depending on local checkouts. Keep
        # development-only shared UI fixes until they are committed upstream.
        packageSource =
          pkgs.runCommand "${cargoToml.package.name}-source"
            {
              nativeBuildInputs = [ pkgs.patch ];
            }
            ''
              mkdir -p "$out/longshot" "$out/videosubextract/crates"
              cp -r ${
                pkgs.lib.fileset.toSource {
                  root = ./.;
                  fileset = pkgs.lib.fileset.unions [
                    ./Cargo.toml
                    ./Cargo.lock
                    ./src
                  ];
                }
              }/. "$out/longshot/"
              cp -r ${iced-source} "$out/iced-dioxus"
              cp ${vse-source}/Cargo.toml "$out/videosubextract/"
              cp -r ${vse-source}/crates/vse-ui "$out/videosubextract/crates/"
              cp -r ${cosmic-source} "$out/libcosmic"
              chmod -R u+w "$out/libcosmic"
              rm -rf "$out/libcosmic/iced"
              cp -r ${cosmic-iced-source} "$out/libcosmic/iced"
              chmod -R u+w "$out"
              patch -d "$out/videosubextract" -p1 < ${./patches/vse-ui-local.patch}
              patch -d "$out/iced-dioxus" -p1 < ${./patches/iced-keyed-column.patch}
            '';
        runtimeLibraries = with pkgs; [
          wayland
          libxkbcommon
          vulkan-loader
          libGL
          libx11
          libxcursor
          libxi
          libxrandr
        ];
      in
      {
        packages.default = rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          version = cargoToml.package.version;
          src = packageSource;
          cargoRoot = "longshot";
          buildAndTestSubdir = "longshot";
          cargoHash = "sha256-vcoGj4QDyM0uCMggvghDQjDdSpq/9n/TLXjflw+wW6c=";
          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.makeWrapper
            pkgs.copyDesktopItems
          ];
          buildInputs = [ pkgs.openssl ];
          desktopItems = [
            (pkgs.makeDesktopItem {
              name = cargoToml.package.name;
              desktopName = "Pagecut";
              genericName = "Image to PDF";
              comment = "Split images into printable PDF pages";
              exec = "${cargoToml.package.name} %f";
              icon = "image-x-generic";
              terminal = false;
              categories = [ "Graphics" ];
              mimeTypes = [
                "image/png"
                "image/jpeg"
              ];
              keywords = [
                "Image"
                "PDF"
                "Print"
              ];
            })
          ];
          postFixup = ''
            wrapProgram "$out/bin/${cargoToml.package.name}" \
              --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath runtimeLibraries}" \
              --prefix PATH : "${pkgs.lib.makeBinPath [ pkgs.zenity ]}"
          '';
          meta.mainProgram = cargoToml.package.name;
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
      }
    );
}
