{
  description = "Audio Mirroring for Linux";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    {
      overlays.default = final: prev: {
        audiomirroring = final.callPackage ./package.nix { };
      };
    }
    // flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        packages = {
          audiomirroring = pkgs.callPackage ./package.nix { };
          default = self.packages.${system}.audiomirroring;
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ self.packages.${system}.audiomirroring ];
        };
      }
    );
}
