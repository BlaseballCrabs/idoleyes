{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-20.09";
  inputs.flake-utils.url = "github:numtide/flake-utils";
  inputs.naersk = {
    url = "github:nmattia/naersk";
    inputs.nixpkgs.follows = "nixpkgs";
  };
  inputs.rust-overlay = {
    url = "github:oxalica/rust-overlay";
    inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { nixpkgs, flake-utils, rust-overlay, naersk, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlay naersk.overlay ];
        };
        inherit (pkgs.rust-bin.stable.latest) rust;
        inherit (pkgs.rust-bin.nightly.latest) cargo;
        naersk-lib = pkgs.naersk.override {
          inherit cargo;
          rustc = rust;
        };
        rust-dev = rust.override {
          extensions = [ "rust-src" "clippy" ];
        };
      in rec {
        packages = {
          idoleyes = naersk-lib.buildPackage {
            name = "idoleyes";
            version = "unstable";
            root = ./.;
            cargoBuildOptions = x: x ++ [ "-p idol_bot" ];
            nativeBuildInputs = with pkgs; [ llvmPackages.llvm pkgconfig ];
            buildInputs = with pkgs; [ stdenv.cc.libc openssl ];
            override = x: (x // {
              LIBCLANG_PATH = "${pkgs.llvmPackages.libclang}/lib";
              preConfigure = ''
              export BINDGEN_EXTRA_CLANG_ARGS="-isystem ${pkgs.clang}/resource-root/include $NIX_CFLAGS_COMPILE"
              '';
            });
          };
        };

        defaultPackage = packages.idoleyes;

        devShell = pkgs.mkShell {
          inputsFrom = packages.idoleyes.builtDependencies;
          nativeBuildInputs = with pkgs; [ sqlite rust-dev ];
        };
      }
    );
}
