{
  lib,
  rustPlatform,
  stdenv,
}:

rustPlatform.buildRustPackage {
  __structuredAttrs = true;

  pname = "flyline";
  version = (lib.importTOML ../Cargo.toml).package.version;

  src = lib.cleanSource ../.;

  # Some deps come from git forks; fetchCargoVendor bundles them under one hash.
  cargoHash = "sha256-PS2whyXd5O1yGDM7Yqzo0TOQFDje+b84/79eJjEBU8k=";

  # Reproducible builds on macOS (Linux needs nothing extra). Timestamps come
  # from SOURCE_DATE_EPOCH (set by Nix, honored by build.rs); these flags fix
  # Mach-O leaks:
  #   --remap-path-prefix  strip the randomised build dir from rustc paths
  #   -install_name        pin LC_ID_DYLIB (else it's the absolute build path)
  #   -reproducible        normalize LC_UUID / ad-hoc signature
  # Exporting RUSTFLAGS overrides .cargo/config.toml, so re-add -undefined
  # dynamic_lookup (flyline resolves Bash symbols at load time).
  preConfigure = lib.optionalString stdenv.hostPlatform.isDarwin ''
    export RUSTFLAGS="--remap-path-prefix=$NIX_BUILD_TOP=/build -C link-arg=-undefined -C link-arg=dynamic_lookup -C link-arg=-Wl,-install_name,@rpath/libflyline.dylib -C link-arg=-Wl,-reproducible''${RUSTFLAGS:+ $RUSTFLAGS}"
  '';

  # The docker_integration_tests need Docker, which the sandbox lacks; skip them.
  checkFlags = [
    "--skip=test_bash_3_2_57"
    "--skip=test_bash_4_4_18"
    "--skip=test_bash_4_4_rc1"
    "--skip=test_bash_5_0"
    "--skip=test_bash_5_3"
  ];

  meta = {
    description = "Bash plugin to replace readline for a modern line editing experience";
    longDescription = ''
      Flyline is a Bash loadable builtin (a dynamic library dlopen()ed by Bash)
      that adds a rich line editor: inline suggestions, fuzzy tab completion,
      configurable keybindings and prompts. Enable it in an interactive shell
      with `enable -f ${placeholder "out"}/lib/libflyline.<ext> flyline`.
    '';
    homepage = "https://github.com/HalFrgrd/flyline";
    license = lib.licenses.gpl3Plus;
    platforms = lib.platforms.unix;
  };
}
