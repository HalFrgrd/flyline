{
  lib,
  bashInteractive,
  rustPlatform,
  stdenv,
}:

rustPlatform.buildRustPackage {
  __structuredAttrs = true;

  pname = "flyline";
  version = (lib.importTOML ../Cargo.toml).package.version;

  src = lib.cleanSource ../.;

  cargoDeps = rustPlatform.importCargoLock {
    lockFile = ../Cargo.lock;
    allowBuiltinFetchGit = true;
  };
  cargoHash = null;

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

  # gcc defaults to gnu2 here, which stamps a GLIBC_ABI_GNU2_TLS dep (glibc >= 2.41) on the
  # library; Bash dlopen()s it against the *host* libc, so that breaks on older distros.
  env = lib.optionalAttrs (stdenv.hostPlatform.isLinux && stdenv.hostPlatform.isx86_64) {
    NIX_CFLAGS_COMPILE = "-mtls-dialect=gnu";
  };

  # The docker_integration_tests need Docker, which the sandbox lacks; skip them.
  checkFlags = [
    "--skip=test_bash_3_2_57"
    "--skip=test_bash_4_4_18"
    "--skip=test_bash_4_4_rc1"
    "--skip=test_bash_5_0"
    "--skip=test_bash_5_3"
  ];

  nativeCheckInputs = [ bashInteractive ];

  postCheck = ''
    library="$(find target -type f -name 'libflyline${stdenv.hostPlatform.extensions.sharedLibrary}' -print -quit)"
    test -n "$library"
    ${bashInteractive}/bin/bash --noprofile --norc -i -c \
      'enable -f "$1" flyline' flyline-nix-check "$library"

    if grep -qa GLIBC_ABI_GNU2_TLS "$library"; then
      echo "libflyline needs GLIBC_ABI_GNU2_TLS; it will not load into a bash on glibc < 2.41" >&2
      exit 1
    fi
  '';

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
