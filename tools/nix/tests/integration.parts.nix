{ lib, ... }: {
  perSystem =
    { pkgs, self', ... }:
    let
      config = import ./configurations.nix {
        inherit lib pkgs;
        clientApp = self'.packages.service;
      };
      inherit (config) nodes;

      # Module Documentation: <nixpkgs>/nixos/lib/testing/nodes.nix
      mkIntegrationTest =
        args:
        pkgs.testers.runNixOSTest (
          {
            name = "integration-test";

            inherit nodes;

            # node.pkgsReadOnly = true;
            globalTimeout = 30;
            sshBackdoor.enable = true;

            # Add stuff only for the interactive test driver.
            interactive.nodes = lib.concatMapAttrs (nodeName: _: {
              "${nodeName}" = { pkgs, ... }: {
                virtualisation.resolution = {
                  x = 1920;
                  y = 1080;
                };
                environment.systemPackages = [
                  pkgs.coreutils
                ];
              };
            }) nodes;

            # References: In nixpkgs repo:
            # Path: `nixos/lib/test-driver/src/test_driver`
            testScript =
              # Python
              ''
                import json
                from dataclasses import dataclass
                from datetime import timedelta

                # serial_stdout_off()

                @dataclass
                class Machines:
                    side_a: Any
                    side_b: Any

                log.info("Starting tests.")
                start_all()

                with subtest("Waiting for stunserver and open UDP port."):
                  stun_server.wait_for_unit("stunserver.service")
                  stun_server.succeed("ss -Hulpn 'sport = :3478' | grep -q 'stunserver'")

                with subtest("Get config file."):
                  status, stdout = side_a.execute("cat \"$CONFIG_FILE\"")
                  t.assertEqual(status, 0, "Could not get configuration file.")
                  cfg = json.loads(stdout)
                  log.info(f"Configuration file:\n{cfg}")

                stun_server_ip = cfg["stun-server"]["interfaces"]["wan"]

                with subtest("Set NAT off and check stunclient fails."):
                    # Drop the 'post' chain on `nixos-nat` table.
                    # So that the packets from `side-a` get dropped in the nixos-fw.
                    nat_a.succeed("nft flush chain ip nixos-nat post")
                    stat, _ = side_a.execute(f"stunclient {stun_server_ip} 3478", timeout=timedelta(seconds=2))
                    t.assertNotEqual(stat, 0, "Stun client still reachable.")
                    log.info("Stun server is not reachable.")

                    log.info("Restart nftables")
                    nat_a.succeed("systemctl restart nftables")

                with subtest(f"Contact stunserver at {stun_server_ip} from 'side_a'."):
                    stat, stdout = side_a.execute(f"stunclient {stun_server_ip} 3478")
                    t.assertEqual(stat, 0, "Could not do binding request.")
                    log.info(f"Binding request response:\n{stdout}")

                with subtest("Check that side-a and side-b have completed successfully."):
                  log.info("Start client-app.")
                  side_a.systemctl("start --no-block client-app.service")
                  side_b.systemctl("start --no-block client-app.service")

                  log.info("Check client-app has completed successfully (in state active).")
                  side_a.wait_for_unit("client-app.service")
                  side_b.wait_for_unit("client-app.service")
              '';
          }
          // args
        );
    in
    {
      packages.test-integration = mkIntegrationTest { globalTimeout = 30; };
      packages.test-integration-ci = mkIntegrationTest { globalTimeout = 600; };
    };
}
