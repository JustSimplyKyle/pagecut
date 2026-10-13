# Longshot

A minimal Iced image preview using the neighboring `vse-ui` crate.
Build and run the packaged application with pinned dependencies:

```sh
nix build
nix run
```

The package includes its graphics libraries and Zenity dialog fallback. It does
not require neighboring checkouts or the development shell.

Development requires `../videosubextract`, `../iced-dioxus`, and `../libcosmic`
checkouts.

Run in the Nix development shell (nightly Rust with Cranelift and mold):

```sh
nix develop
cargo run
```

The image starts zoomed out so the A4 limit and about 100 screen pixels below
it are visible without scrolling. The rest of the image scrolls vertically.
Use **Open image…** to choose a PNG or JPEG through the native file dialog.
The shell provides D-Bus and Zenity for RFD's Linux portal and fallback dialogs.
After shell dependency changes, run `direnv reload` and restart the app.
You can also start with an image path: `cargo run -- "畫作 3.png"`.
Image-loading failures show an error screen with a Retry button.

The Justfile carries over VideoSubExtract's Dioxus workflows: `just dev` for
hot reload, `just dev-release` for release hot reload, and `just release` for a
release build and launch. The Nix shell provides the patched Dioxus CLI needed
for Cranelift hot patches.

Build the web release with `just build-web`, then deploy it to
https://pagecut.pages.dev with `just publish`. Publishing uses Wrangler through
Nix and requires a Cloudflare login (`nix shell nixpkgs#wrangler --command
wrangler login`). It copies `_headers` into the build output so browser threads
can run with cross-origin isolation.

A translucent tint overlays the portion that fits on one portrait A4 page with
10 mm margins. A draggable orange rule marks the proposed cut. The guide scales
with the image and scrolls with it.
The cut label occupies a separate gap at the A4 boundary, pushing the remaining
image down without covering any content.

Click an image gap to move the rule there without saving or advancing. You can
also drag the rule, check the **cut detail** view, and
use the one-pixel buttons for fine adjustments. **Confirm cut** saves the cut
and advances; clicking or releasing a drag only adjusts the rule.
Use **↑/↓** to move the rule by one image pixel, or **Shift+↑/↓** for five pixels.
The detail view contains the full image width and shows the vertical area around
the cut, scaling to fit without cropping the left or right sides.

The sidebar shows each saved page and the remaining image. Select a thumbnail
to revisit a page, or scroll up in the main preview and select an earlier blue
rule. **Save cut** adjusts that boundary while preserving later cuts. The guide
stays within limits that keep both adjacent pages within A4 size.

After every page is confirmed, use **Export PDF…** to choose a destination.
Each confirmed slice becomes a portrait A4 page with 10 mm margins, preserving
the source resolution and aspect ratio. Transparent pixels print on white.
Save any pending cut edits before exporting. Cancelling a dialog keeps your
current work, and export errors let you return to it.

Cuts are kept in memory; saved cut state is not implemented yet.
The original `longshot-a4.sh` remains available.
