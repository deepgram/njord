{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # Define Rust toolchain with musl target
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          targets = [ "x86_64-unknown-linux-musl" ];
        };
        
        njord = pkgs.rustPlatform.buildRustPackage {
          pname = "njord";
          version = "0.1.0";
          src = ./.;
          
          cargoLock.lockFile = ./Cargo.lock;

          # For musl builds
          CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
          CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = "${pkgs.pkgsStatic.stdenv.cc}/bin/cc";
        };
      in
      {
        packages.default = njord;
        apps.default = flake-utils.lib.mkApp {
          drv = njord;
        };
        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.pkgsStatic.stdenv.cc
          ];

          CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
          CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = "${pkgs.pkgsStatic.stdenv.cc}/bin/${pkgs.pkgsStatic.stdenv.cc.targetPrefix}cc";

          shellHook = ''
            export CC_x86_64_unknown_linux_musl="${pkgs.pkgsStatic.stdenv.cc}/bin/${pkgs.pkgsStatic.stdenv.cc.targetPrefix}cc"
          '';
        };
      });
}
