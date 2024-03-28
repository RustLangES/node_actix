let
  inherit
    (builtins)
    currentSystem
    fromJSON
    readFile
    ;
  getFlake = name:
    with (fromJSON (readFile ./flake.lock)).nodes.${name}.locked; {
      inherit rev;
      outPath = fetchTarball {
        url = "https://github.com/${owner}/${repo}/archive/${rev}.tar.gz";
        sha256 = narHash;
      };
    };
in
  {
    system ? currentSystem,
    pkgs ? import (getFlake "nixpkgs") {localSystem = {inherit system;};},
    lib ? pkgs.lib,
    crane,
    cranix,
    fenix,
    stdenv ? pkgs.stdenv,
    ...
  }: let
    # fenix: rustup replacement for reproducible builds
    toolchain = fenix.${system}.fromToolchainFile {
      file = ./rust-toolchain.toml;
      sha256 = "sha256-e4mlaJehWBymYxJGgnbuCObVlqMlQSilZ8FljG9zPHY=";
    };
    # crane: cargo and artifacts manager
    craneLib = crane.${system}.overrideToolchain toolchain;
    # cranix: extends crane building system with workspace bin building and Mold + Cranelift integrations
    cranixLib = craneLib.overrideScope' (cranix.${system}.craneOverride);

    # buildInputs for Examples
    buildInputs = [ ];

    src = lib.cleanSourceWith {
      src = craneLib.path ./.;
      filter = craneLib.filterCargoSources;
    };

    # Lambda for build packages with cached artifacts
    commonArgs = {}:
      {
        inherit src;
        doCheck = false;
        isLibTarget = true;
      };
      defaultApp = cranixLib.buildCranixBundle (commonArgs);
  in {
    # `nix check`
    checks = {
      node-test = {
      };
      cargo-clippy = craneLib.cargoClippy ({
        inherit src;
        cargoClippyExtraArgs = "-- -D warnings";
      });
      cargo-fmt = craneLib.cargoFmt {
        inherit src;
        cargoExtraArgs = "--all";
      };
    };

    # `nix build`
    packages.default = {
    };

    # `nix run`
    apps.default = {
    };

    # `nix develop`
    devShells.default = cranixLib.devShell {
      packages = with pkgs;
        [
          bun
          toolchain
          pkg-config
          cargo-release
        ]
        ++ buildInputs;
      LD_LIBRARY_PATH = lib.makeLibraryPath buildInputs;
    };
  }
