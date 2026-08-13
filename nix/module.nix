{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.flyline;

  inherit (lib.options) mkEnableOption;
  inherit (lib.modules) mkIf;

  flyline = pkgs.callPackage ./package.nix { };
in
{
  options.programs.flyline.enable = mkEnableOption "flyline integration into bash shell";
  config = mkIf cfg.enable {
    environment.systemPackages = [ flyline ];

    programs.bash.interactiveShellInit = ''
      enable flyline 2>/dev/null || enable -f ${flyline}/lib/libflyline.so flyline
    '';
  };
}
