{
  description = "Bash plugin to replace readline for a modern line editing experience";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      nixosModules = {
        flyline = import ./nix/module.nix;
        default = self.nixosModules.flyline;
      };

      packages = forAllSystems (pkgs: rec {
        flyline = pkgs.callPackage ./nix/package.nix { };
        default = flyline;
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt-rfc-style);
    };
}
