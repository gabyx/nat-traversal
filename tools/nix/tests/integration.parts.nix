{ lib, ... }: {
  perSystem =
    { pkgs, ... }:
    let
      config = import ./configurations.nix { inherit lib pkgs; };
      inherit (config) nodes;
    in
    {
      # Documentation:
      # Module Documentation: nixos/lib/testing/nodes.nix

      packages.integration-test = pkgs.testers.runNixOSTest {
        name = "integration-test";

        # node.pkgsReadOnly = true;
        globalTimeout = 30;
        sshBackdoor.enable = true;

        inherit nodes;

        # Add stuff only for the interactive system.
        interactive.nodes.stun-server = { pkgs, ... }: {
          environment.systemPackages = [
            pkgs.coreutils
          ];
        };

        testScript =
          # Python
          ''
            serial_stdout_off()

            from dataclasses import dataclass
            @dataclass
            class Machines:
                side_a: Any
                side_b: Any

            log.info("Starting tests.")
            start_all()

            with subtest("Waiting for stunserver."):
              stun_server.wait_for_unit("stunserver.service")
              stun_server.wait_for_open_port(3478)

            with subtest("Contacting stunserver."):
              log.info("Contacting stunserver.")

              # https://docs.python.org/3/library/unittest.html
              # t.assertIn("asdf")

          '';
      };
    };
}
