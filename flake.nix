{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        
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
      });
}
