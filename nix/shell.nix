{ pkgs }:

pkgs.mkShell {
  strictDeps = true;

  packages = with pkgs; [
    bashInteractive
    cargo
    clippy
    git
    rustc
    rustfmt
  ];

  nativeBuildInputs = [ pkgs.pkg-config ];
}
