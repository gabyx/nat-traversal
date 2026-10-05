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

        # References: In nixpkgs repo:
        # Path: `nixos/lib/test-driver/src/test_driver`
        testScript =
          # Python
          ''
            import json
            from dataclasses import dataclass

            serial_stdout_off()

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
            with subtest(f"Contact stunserver at {stun_server_ip} from 'nat_a'."):
                stat, stdout = nat_a.execute(f"stunclient {stun_server_ip} 3478")
                t.assertEqual(status, 0, "Could not do binding request.")
                log.info(f"Binding request response:\n{stdout}")

            with subtest("Set NAT off and check stunclient fails."):
                # Drop the 'post' chain on `nixos-nat` table
                nat_a.succeed("nft flush chain ip nixos-nat post")
                stat, _ = side_a.execute(f"stunclient {stun_server_ip} 3478", timeout=2)
                t.assertEqual(status, 1, "Stun client still reachable.")

                log.info("Restart nftables")
                nat_a.succeed("systemctl restart nftables")

            with subtest(f"Contact stunserver at {stun_server_ip} from 'side_a'."):
                stat, stdout = side_a.execute(f"stunclient {stun_server_ip} 3478")
                t.assertEqual(status, 0, "Could not do binding request.")
                log.info(f"Binding request response:\n{stdout}")
          '';
      };
    };
}
