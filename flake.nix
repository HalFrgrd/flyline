{
  description = "Bash plugin to replace readline for a modern line editing experience";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      inherit (nixpkgs) lib;

      pkgsFor = system: nixpkgs.legacyPackages.${system} or (import nixpkgs { inherit system; });

      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      forAllSystems = f: lib.genAttrs supportedSystems (system: f (pkgsFor system));
    in
    {
      overlays.default = final: _: {
        flyline = final.callPackage ./nix/package.nix { };
      };

      nixosModules = {
        flyline = import ./nix/module.nix;
        default = self.nixosModules.flyline;
      };

      packages = forAllSystems (pkgs: rec {
        flyline = pkgs.callPackage ./nix/package.nix { };
        default = flyline;
      });

      checks = lib.genAttrs supportedSystems (system: {
        flyline = self.packages.${system}.flyline;
      });

      devShells = forAllSystems (pkgs: {
        default = import ./nix/shell.nix { inherit pkgs; };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt-rfc-style);
    };
}
