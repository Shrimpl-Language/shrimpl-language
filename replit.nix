{ pkgs }: {
  deps = [
    pkgs.cargo
    # pkgs.rustc      # make sure these are commented out or removed
    # pkgs.cargo
    pkgs.rust-analyzer
    pkgs.pkg-config
    pkgs.rustup
    pkgs.bashInteractive
  ];
}
