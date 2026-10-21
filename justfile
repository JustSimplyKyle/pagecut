# `just dev` runs the app; `just dev <example>` runs examples/<example>.rs.
dev example="":
    dx serve --platform linux --hot-patch --features hot-patch {{ if example == "" { "" } else { "--example " + example } }}
dev-release:
    dx serve --platform linux --release --hot-patch --features hot-patch
release:
    dx serve --platform linux --release
dev-web:
    dx serve --platform web --profile web --hot-patch false --cross-origin-policy --cargo-args="-Zbuild-std=std,panic_abort"
build-web:
    dx build --platform web --release --cargo-args="-Zbuild-std=std,panic_abort"
publish:
    cp _headers target/dx/pagecut/release/web/public/_headers
    nix shell nixpkgs#wrangler --command wrangler pages deploy target/dx/pagecut/release/web/public --project-name pagecut --branch main --commit-dirty=true
# Unpack loki-file-access and apply the same fixes as the flake.
loki-file-access:
    rm -rf crates/loki-file-access
    mkdir -p crates/loki-file-access
    curl -sL https://static.crates.io/crates/loki-file-access/loki-file-access-0.1.1.crate | tar xz --strip-components=1 -C crates/loki-file-access
    patch -d crates/loki-file-access -p1 < patches/loki-file-access-fixes.patch
# Run inside `nix develop '.#android'`; a bare `--device` skips booting an emulator.
dev-android:
    dx serve --platform android --device
# Create the emulator image that `dev-android` boots when no device is connected.
android-avd:
    echo no | avdmanager create avd --force --name pagecut --package "system-images;android-34;default;x86_64" --device pixel_7
build-android:
    dx build --platform android --release --target aarch64-linux-android
