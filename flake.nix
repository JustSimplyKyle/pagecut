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
      url = "github:JustSimplyKyle/iced/91dadfdd6952ffebf621642b88271d6b4ba78239";
      flake = false;
    };
    vse-source = {
      url = "github:JustSimplyKyle/videosubextract/ce17f5e1a1600890f8d5687e04f7fe599de30a09";
      flake = false;
    };
    cosmic-source = {
      url = "github:pop-os/libcosmic/d3781f9c0bc4bb1c1d3348ae3017f65b89e8b2f2";
      flake = false;
    };
    loki-file-access-source = {
      url = "tarball+https://static.crates.io/crates/loki-file-access/loki-file-access-0.1.1.crate";
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
      loki-file-access-source,
    }:
    utils.lib.eachDefaultSystem (
      system:
      let
        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
          # The Android SDK and NDK are unfree and require license acceptance.
          config.allowUnfree = true;
          config.android_sdk.accept_license = true;
        };
        rustToolchain = pkgs.rust-bin.nightly.latest.default.override {
          targets = [
            "wasm32-unknown-unknown"
            "aarch64-linux-android"
            "x86_64-linux-android"
          ];
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
            ./patches/dioxus-cli-wasm-worker-init.patch
            ./patches/dioxus-cli-wasm-asset-tls.patch
          ];
        });
        # vse's root manifest is also a package; keep only the workspace that vse-ui inherits from.
        vseManifest = builtins.fromTOML (builtins.readFile "${vse-source}/Cargo.toml");
        vseWorkspaceManifest = (pkgs.formats.toml { }).generate "Cargo.toml" {
          workspace = {
            inherit (vseManifest.workspace) members resolver dependencies;
          };
        };
        # Recreate the path layout without depending on local checkouts.
        packageSource =
          pkgs.runCommand "${cargoToml.package.name}-source"
            {
              nativeBuildInputs = [ pkgs.patch ];
            }
            ''
              mkdir -p "$out/pagecut" "$out/videosubextract/crates"
              cp -r ${
                pkgs.lib.fileset.toSource {
                  root = ./.;
                  fileset = pkgs.lib.fileset.unions [
                    ./Cargo.toml
                    ./Cargo.lock
                    ./crates
                    ./src
                  ];
                }
              }/. "$out/pagecut/"
              cp -r ${iced-source} "$out/iced-dioxus"
              cp ${vseWorkspaceManifest} "$out/videosubextract/Cargo.toml"
              cp -r ${vse-source}/crates/vse-ui "$out/videosubextract/crates/"
              cp -r ${cosmic-source} "$out/libcosmic"
              chmod -R u+w "$out/libcosmic"
              rm -rf "$out/libcosmic/iced"
              cp -r ${cosmic-iced-source} "$out/libcosmic/iced"
              chmod -R u+w "$out"
              cp -r ${loki-file-access-source} "$out/pagecut/crates/loki-file-access"
              chmod -R u+w "$out/pagecut/crates/loki-file-access"
              patch -d "$out/pagecut/crates/loki-file-access" -p1 < ${./patches/loki-file-access-fixes.patch}
            '';
        # Match the compile SDK and build tools expected by dx's Gradle template.
        androidComposition = pkgs.androidenv.composeAndroidPackages {
          platformVersions = [ "34" ];
          buildToolsVersions = [ "34.0.0" ];
          includeNDK = true;
          ndkVersions = [ "29.0.14206865" ];
          # `dx serve` boots the first AVD it finds; see `just android-avd`.
          includeEmulator = true;
          includeSystemImages = true;
          systemImageTypes = [ "default" ];
          abiVersions = [ "x86_64" ];
        };
        androidSdk = androidComposition.androidsdk;
        androidSdkRoot = "${androidSdk}/libexec/android-sdk";
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
          cargoRoot = "pagecut";
          buildAndTestSubdir = "pagecut";
          cargoHash = "sha256-jIzVQrHcdRoILKhosU/jkVY0/IFZFc5EENITjGJtXo8=";
          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.makeWrapper
            pkgs.copyDesktopItems
          ];
          buildInputs = [ pkgs.openssl ];
          COSMIC_ICONS = "${pkgs.cosmic-icons}/share";
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
            # Must match iced's pinned wasm-bindgen dependency exactly.
            pkgs.wasm-bindgen-cli_0_2_127
            pkgs.esbuild
            pkgs.binaryen
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
          COSMIC_ICONS = "${pkgs.cosmic-icons}/share";
          # LIBCLANG_PATH="${pkgs.libclang.lib}/lib";

        };
        devShells.android = self.devShells.${system}.default.overrideAttrs (old: {
          buildInputs = old.buildInputs ++ [
            androidSdk
            pkgs.jdk17
          ];
          ANDROID_HOME = androidSdkRoot;
          ANDROID_SDK_ROOT = androidSdkRoot;
          ANDROID_NDK_HOME = "${androidSdkRoot}/ndk-bundle";
          JAVA_HOME = pkgs.jdk17.home;
          # Gradle's downloaded aapt2 is dynamically linked and cannot run on NixOS.
          GRADLE_OPTS = "-Dorg.gradle.project.android.aapt2FromMavenOverride=${androidSdkRoot}/build-tools/34.0.0/aapt2";
        });
      }
    );
}
