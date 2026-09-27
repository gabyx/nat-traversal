{ ... }: {
  perSystem =
    { pkgs, ... }:
    let
      config = import ./configurations.nix { };
      inherit (config) nodes;
    in
    {
      packages.integration-test = pkgs.testers.runNixOSTest {
        name = "integration-test";

        inherit nodes;

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

            stun_server.succeed("stunserver --primaryinterface 0.0.0.0 &")

            log.info("Contacting stunserver.")
            # nat_a.succeed("echo 'Get IP'; stunclient ${nodes.stun-server.networking.primaryIPAddress}")

          '';
      };
    };
}
