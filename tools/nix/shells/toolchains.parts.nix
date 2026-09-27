{
  ...
}:
{
  perSystem =
    {
      self',
      pkgs,
      ...
    }:
    let
      format = [
        {
          packages = [
            self'.packages.treefmt
          ];
        }
      ];

      changelog = [
        {
          packages = [
            self'.packages.generate-changelog
          ];
        }
      ];

      general = format ++ [
        {
          packages = [
            self'.packages.bootstrap
            pkgs.stuntman
          ];
        }
      ];
    in
    {
      # Define some toolchains.
      repo.toolchains = {
        inherit format changelog general;
      };
    };
}
