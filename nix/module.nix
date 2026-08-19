{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.flyline;

  inherit (lib.options) mkEnableOption mkOption;
  inherit (lib.modules) mkAfter mkIf;
  inherit (lib.types) package;

  defaultPackage = pkgs.callPackage ./package.nix { };
  libraryName = "libflyline${pkgs.stdenv.hostPlatform.extensions.sharedLibrary}";
in
{
  options.programs.flyline = {
    enable = mkEnableOption "flyline integration into bash shell";

    package = mkOption {
      type = package;
      default = defaultPackage;
      description = "The flyline package to load into Bash.";
    };
  };

  config = mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];

    programs.bash.interactiveShellInit = mkAfter ''
      enable -f ${cfg.package}/lib/${libraryName} flyline
    '';
  };
}
