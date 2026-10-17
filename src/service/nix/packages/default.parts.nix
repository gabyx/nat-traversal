{ self, ... }: {
  perSystem = { config, pkgs, ... }: {
    packages.service = pkgs.callPackage ./service.nix {
      inherit self;
      rustPlatform = config.repo.pinned.rust.release.platform;
    };
  };
}
