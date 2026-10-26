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
