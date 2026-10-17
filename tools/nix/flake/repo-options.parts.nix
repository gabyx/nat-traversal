{
  lib,
  flake-parts-lib,
  ...
}:
let
  inherit (flake-parts-lib) mkPerSystemOption;
  inherit (lib) mkOption types;
in
{
  # Creates a `perSystem.toolchains.<key> = [ <devenvModule> ]` option.
  options.perSystem = mkPerSystemOption (
    {
      ...
    }:
    {
      options.repo = {
        toolchains = mkOption {
          description = "Attrset of toolchain definitions keyed by toolchain name, where each value is a list of devenv modules.";
          default = { };
          type = types.attrsOf (types.listOf types.deferredModule);
        };

        pinned = mkOption {
          description = "Pinned packages and stuff.";
          default = { };
          type = types.attrsOf types.raw;
        };
      };
    }
  );

  config = {
    perSystem = { config, ... }: {
      legacyPackages.repo = config.repo;
    };
  };
}
