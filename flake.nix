{
  description = "Incus Manager – Tauri desktop app for managing Incus containers";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    # Incus source (for docs + version string).
    incus-src = {
      url = "github:lxc/incus";
      flake = false;
    };

    # The Incus UI submodule (zabbly/incus-ui-canonical).
    incus-ui-src = {
      url = "github:zabbly/incus-ui-canonical";
      flake = false;
    };

    # canonical-sphinx-extensions (Python package needed for docs build)
    canonical-sphinx-extensions-src = {
      url = "github:canonical/canonical-sphinx-extensions";
      flake = false;
    };

    # swagger-ui (cloned by Sphinx conf.py at build time — prefetch here)
    swagger-ui-src = {
      url = "github:swagger-api/swagger-ui";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, incus-src, incus-ui-src, canonical-sphinx-extensions-src, swagger-ui-src }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      lib = pkgs.lib;

      # ── Python environment for Sphinx docs ───────────────────────────────────

      canonical-sphinx-extensions = pkgs.python3Packages.buildPythonPackage {
        pname = "canonical-sphinx-extensions";
        version = "0-unstable";
        src = canonical-sphinx-extensions-src;
        format = "pyproject";
        nativeBuildInputs = [ pkgs.python3Packages.setuptools ];
        propagatedBuildInputs = with pkgs.python3Packages; [ sphinx beautifulsoup4 gitpython ];
        doCheck = false;
      };

      sphinxPython = pkgs.python3.withPackages (ps: with ps; [
        sphinx
        furo
        myst-parser
        sphinx-copybutton
        sphinx-design
        sphinx-tabs
        sphinxext-opengraph
        sphinx-notfound-page
        sphinx-reredirects
        sphinx-remove-toctrees
        sphinxcontrib-jquery
        gitpython
        linkify-it-py
        pyspelling
        docutils
        canonical-sphinx-extensions
      ]);

      # ── Docs derivation ──────────────────────────────────────────────────────

      incus-docs = pkgs.stdenv.mkDerivation {
        pname = "incus-docs";
        version = "0";
        src = incus-src;

        nativeBuildInputs = [ sphinxPython pkgs.incus pkgs.go pkgs.git ];

        buildPhase = ''
          # conf.py calls: go env GOPATH → $GOPATH/bin/incus
          export HOME=$TMPDIR
          mkdir -p .gopath/bin
          ln -s ${pkgs.incus}/bin/incus .gopath/bin/incus
          export GOPATH=$(pwd)/.gopath

          # conf.py tries to git-clone swagger-ui; pre-populate it
          mkdir -p doc/.sphinx/deps
          cp -r ${swagger-ui-src} doc/.sphinx/deps/swagger-ui

          # Use the flake input's commit date so Sphinx/Furo shows a real
          # date instead of 1980 (Nix sandbox default).
          export SOURCE_DATE_EPOCH=${toString incus-src.lastModified}

          sphinx-build -b html -q doc $out
        '';

        dontInstall = true;
      };

      # ── UI SPA derivation (incus-ui-canonical) ──────────────────────────────

      uiYarnDeps = pkgs.fetchYarnDeps {
        yarnLock = "${incus-ui-src}/yarn.lock";
        hash = "sha256-08G3jYj7N9h6aBnqwGQQtpYOP/wP/k2VRY7dgmpxXZw=";
      };

      incus-ui = pkgs.stdenv.mkDerivation {
        pname = "incus-ui";
        version = "0";
        src = incus-ui-src;

        nativeBuildInputs = [
          pkgs.yarn
          pkgs.fixup-yarn-lock
          pkgs.nodejs
        ];

        configurePhase = ''
          export HOME=$TMPDIR
          yarn config --offline set yarn-offline-mirror $yarnOfflineCache
          fixup-yarn-lock yarn.lock
          yarn install --offline --frozen-lockfile --ignore-scripts --no-progress
          patchShebangs node_modules
        '';

        buildPhase = ''
          npx vite build
          cp build/ui/index.html build/index.html
          mkdir -p build/ui/monaco-editor
          cp -R node_modules/monaco-editor/min build/ui/monaco-editor/
        '';

        installPhase = ''
          cp -r build/ui $out
        '';

        yarnOfflineCache = uiYarnDeps;
      };

      # ── Shell derivation (React settings wrapper) ───────────────────────────

      shellYarnDeps = pkgs.fetchYarnDeps {
        yarnLock = ./yarn.lock;
        hash = "sha256-C4fabtbTLQSVrfSwwWSbgSKraY3jLfUVmDl3QzxONBY=";
      };

      incus-shell = pkgs.stdenv.mkDerivation {
        pname = "incus-shell";
        version = "0";
        src = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./package.json
            ./yarn.lock
            ./tsconfig.json
            ./vite.config.ts
            ./index.html
            ./src
          ];
        };

        nativeBuildInputs = [
          pkgs.yarn
          pkgs.fixup-yarn-lock
          pkgs.nodejs
        ];

        configurePhase = ''
          export HOME=$TMPDIR
          yarn config --offline set yarn-offline-mirror $yarnOfflineCache
          fixup-yarn-lock yarn.lock
          yarn install --offline --frozen-lockfile --ignore-scripts --no-progress
          patchShebangs node_modules
        '';

        buildPhase = ''
          npx vite build
        '';

        installPhase = ''
          cp -r dist $out
        '';

        yarnOfflineCache = shellYarnDeps;
      };

      # ── Version metadata ─────────────────────────────────────────────────────

      incusVersion = let
        flexGo = builtins.readFile "${incus-src}/internal/version/flex.go";
        match = builtins.match ".*var Version = \"([^\"]+)\".*" flexGo;
      in if match != null then builtins.head match else "unknown";

      incusSrcRev = incus-src.shortRev or "unknown";
      incusUiRev = incus-ui-src.shortRev or "unknown";

      # ── Main Tauri/Rust package ──────────────────────────────────────────────

      mkIncusManager = { nativeTitlebar ? false }:
        let
          versionGo = ''
            package version

            var Version = "${incusVersion}"
          '';
        in
        pkgs.rustPlatform.buildRustPackage {
          pname = "incus-manager";
          version = "0.1.0";

          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./src-tauri
            ];
          };

          cargoRoot = "src-tauri";
          buildAndTestSubdir = "src-tauri";
          cargoLock.lockFile = ./src-tauri/Cargo.lock;

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          buildInputs = with pkgs; [
            openssl
            gtk3
            glib
            cairo
            pango
            gdk-pixbuf
            dbus
            webkitgtk_4_1
            libsoup_3
            libayatana-appindicator
          ];

          # libappindicator-sys dlopen's the .so at build time
          LD_LIBRARY_PATH = lib.makeLibraryPath [ pkgs.libayatana-appindicator ];

          # Link pre-built frontend assets before cargo runs.
          # build.rs looks for these relative to $CARGO_MANIFEST_DIR/..
          # Stay at the project root — cargoBuildHook handles cd into src-tauri.
          preBuild = ''
            # UI SPA (embedded via rust-embed)
            mkdir -p incus-ui-canonical/build
            ln -s ${incus-ui} incus-ui-canonical/build/ui

            # Documentation HTML (embedded via rust-embed)
            ln -s ${incus-docs} incus-docs-build

            # Shell / Tauri frontend dist
            ln -s ${incus-shell} dist

            # Version metadata for build.rs
            echo "${incusSrcRev}" > .docs-commit
            echo "${incusUiRev}" > .ui-commit

            mkdir -p incus-src/internal/version
            echo ${lib.escapeShellArg versionGo} > incus-src/internal/version/flex.go
          '';

          cargoBuildFlags =
            lib.optionals nativeTitlebar [ "--features" "native-titlebar" ];

          doCheck = false;

          postInstall = ''
            mv $out/bin/tauri-incus $out/bin/incus-manager

            mkdir -p $out/share/applications
            cat > $out/share/applications/incus-manager.desktop <<DESKTOP
[Desktop Entry]
Name=Incus Manager
Comment=Desktop client for Incus containers
Exec=$out/bin/incus-manager
Icon=incus-manager
Type=Application
Categories=System;
DESKTOP

            for size in 32 64 128; do
              install -Dm644 "src-tauri/icons/''${size}x''${size}.png" \
                "$out/share/icons/hicolor/''${size}x''${size}/apps/incus-manager.png"
            done
            install -Dm644 "src-tauri/icons/128x128@2x.png" \
              "$out/share/icons/hicolor/256x256/apps/incus-manager.png"
          '';

          meta = with lib; {
            description = "Desktop app for managing Incus containers and VMs";
            license = licenses.asl20;
            platforms = [ "x86_64-linux" ];
            mainProgram = "incus-manager";
          };
        };

    in {
      packages.${system} = rec {
        default = incus-manager;
        incus-manager = mkIncusManager {};
        incus-manager-with-titlebar = mkIncusManager { nativeTitlebar = true; };
      };

      overlays.default = final: prev: {
        incus-manager = self.packages.${final.system}.default;
      };
    };
}
