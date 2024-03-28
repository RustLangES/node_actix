{
  description = "VirtualJoystick Bevy lib";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    cranix.url = "github:Lemin-n/cranix";
    crane.url = "github:ipetkov/crane";
    fenix.url = "github:nix-community/fenix";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    ...
  } @ inputs:
    flake-utils.lib.eachSystem (flake-utils.lib.defaultSystems) (
      system: let
        libBundle = import ./. {
          inherit system;
          pkgs = nixpkgs.legacyPackages.${system};
          crane = inputs.crane.lib;
          cranix = inputs.cranix.lib;
          fenix = inputs.fenix.packages;
        };
      in {
        inherit (libBundle) apps devShells;
      }
    );
}
