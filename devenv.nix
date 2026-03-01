{ pkgs, lib, ... }:

{
  packages = with pkgs; [
    # Rust toolchain
    cargo
    rustc
    rustfmt
    clippy
    rust-analyzer

    # Node.js + Yarn
    nodejs_22
    yarn

    # Tauri CLI
    cargo-tauri

    # Tauri system deps (Linux / WebKit)
    pkg-config
    openssl
    gtk3
    cairo
    pango
    gdk-pixbuf
    glib
    dbus
    webkitgtk_4_1
    libsoup_3
    libayatana-appindicator

    # Python + Go + Incus binary (for building the Sphinx docs)
    python3
    go
    incus

    # Icon generation
    resvg
    icoutils
    libicns

    # Misc build tools
    curl
    file
    git
  ];

  env.LD_LIBRARY_PATH = lib.makeLibraryPath (with pkgs; [
    libayatana-appindicator
    webkitgtk_4_1
    gtk3
    glib
    cairo
    pango
    gdk-pixbuf
    dbus
    openssl
    libsoup_3
  ]);

  env.PKG_CONFIG_PATH = lib.makeSearchPathOutput "dev" "lib/pkgconfig" (with pkgs; [
    openssl
    gtk3
    cairo
    pango
    gdk-pixbuf
    glib
    dbus
    webkitgtk_4_1
    libsoup_3
    libayatana-appindicator
  ]);

  env.WEBKIT_DISABLE_COMPOSITING_MODE = "1";

  enterShell = ''
    echo ""
    echo "Tauri v2 dev environment"
    echo "  rustc $(rustc --version)"
    echo "  cargo $(cargo --version)"
    echo "  node $(node --version)  yarn $(yarn --version)"
    echo ""
    echo "Run:  cargo tauri dev"
    echo ""
  '';
}
