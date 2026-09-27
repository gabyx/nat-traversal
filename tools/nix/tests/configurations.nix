{ ... }:
let
  # NOTE: https://applicative.systems/nixos-test-driver-manual/

  # Glossary:
  # LAN: Local Area Network  -- private address space, behind a NAT.
  # WAN: Wide Area Network   -- the "public" side, past the NAT.
  # Frame: One Ethernet packet as it goes over the wire: destination MAC, source MAC, a type field, the payload (your IP packet). Bytes on a cable.
  # Layer 2: The Ethernet layer (OSI Stack), where MAC addresses live. Layer 3 is IP. A switch works at layer 2; a router works at layer 3.
  # Segment (or broadcast domain): One set of machines that can hear each other's frames directly, without a router.
  # Tag: In a real 802.1Q VLAN, 4 extra bytes inserted into the frame header carrying a VLAN id.
  #      It lets one physical cable carry several segments, kept apart by that number.

  # Each number below is one separate virtual network. The NixOS test driver
  # starts a `vde_switch` process (Virtual Distributed Ethernet -- a userspace
  # Ethernet switch) per number, and a VM's interface plugs into exactly one of
  # them. Despite the option being called `vlan`, no IEEE 802.1Q tag is
  # involved: these are simply three unconnected networks, not one wire carrying
  # three tagged streams.
  #
  # Consequence: a node on vlan 1 cannot reach a node on vlan 3 at all. The only
  # path between them is a node with an interface on both, which forwards
  # packets. That node is the NAT router -- so every packet between side-a and
  # the WAN must pass through nat-a, by construction.
  vlans = {
    lan-a = 1; # Local Area Network for side-a and nat-a.
    lan-b = 2; # Local Area Network for side-b and nat-b.
    wan = 3; # Wide Area Network for the stun-server, nat-a and nat-b.
  };

  nodes = {

    #   NODE                    NODE                        NODE                    NODE
    #  ┌────────┐  vlan 1     ┌────────┐     vlan 3       ┌────────┐  vlan 2      ┌────────┐
    #  │ side-a ├─────────────┤ nat-a  ├─────────┬────────┤ nat-b  ├──────────────┤ side-b │
    #  └────────┘             └────────┘         │        └────────┘              └────────┘
    #     lan                 lan      wan       │       wan      lan                lan
    # 192.168.1.3     192.168.1.1  192.168.3.1   │  192.168.3.2  192.168.2.2    192.168.2.4
    #                                            │
    #   "LAN A"              gateway   "public"  │   "public"  gateway            "LAN B"
    #                        for A     addr of A │   addr of B for B
    #                                     ┌──────┴──────┐
    #                                     │ stun-server │  NODE
    #                                     └─────────────┘
    #                                           wan
    #                                      192.168.3.5
    #
    # Addresses are assigned as 192.168.<vlan>.<nodeNumber>, where nodeNumber is
    # the node's index in `attrNames nodes` (alphabetical, from 1). It is
    # readOnly, so renaming a key shifts every address -- never write an address
    # literal, read it back via `nodes.<name>.networking.primaryIPAddress`.

    side-a =
      { nodes, ... }:
      {
        virtualisation.interfaces = {
          lan = {
            vlan = vlans.lan-a;
            assignIP = true;
          };
        };
        networking.firewall.enable = false; # Test nat-a's rules, not side-a's.
        networking.defaultGateway = {
          address = nodes.nat-a.networking.primaryIPAddress;
          interface = "lan";
        };
      };

    # The router on side A.
    nat-a = { ... }: {
      virtualisation.interfaces = {
        lan = {
          vlan = vlans.lan-a;
          assignIP = true;
        };
        wan = {
          vlan = vlans.wan;
          assignIP = true;
        };
      };

      # No default gateway: `wan` is directly attached to the vlan the
      # stun-server and nat-b sit on.
      #
      # TODO(M2.5): networking.nat -- internal `lan`, external `wan`.
    };

    side-b =
      { nodes, ... }:
      {
        virtualisation.interfaces = {
          lan = {
            vlan = vlans.lan-b;
            assignIP = true;
          };
        };
        networking.firewall.enable = false; # Test nat-b's rules, not side-b's.
        networking.defaultGateway = {
          address = nodes.nat-b.networking.primaryIPAddress;
          interface = "lan";
        };
      };

    # The router on side B.
    nat-b = { ... }: {
      virtualisation.interfaces = {
        lan = {
          vlan = vlans.lan-b;
          assignIP = true;
        };
        wan = {
          vlan = vlans.wan;
          assignIP = true;
        };
      };
      # TODO(M2.5): networking.nat -- internal `lan`, external `wan`.
    };

    # A relay server (M7) is out of scope here. Signaling (M3) goes over the
    # shared folder: the driver mounts one host directory at /tmp/shared on
    # *every* node (see nixos/modules/virtualisation/qemu-vm.nix, the `shared`
    # entry of virtualisation.sharedDirectories).

    stun-server =
      { pkgs, lib, ... }:
      {
        virtualisation.interfaces = {
          wan = {
            vlan = vlans.wan;
            assignIP = true;
          };
        };
        # No default gateway: both routers are on this same segment.

        environment.systemPackages = [ pkgs.stuntman ]; # Also gives this node `stunclient`.

        systemd.services.stunserver = {
          description = "STUN server (stuntman)";

          # `network-online.target` is only reached once an address is actually
          # configured on `wan`; without it the unit can start while the
          # interface is still address-less.
          wants = [ "network-online.target" ];
          after = [ "network-online.target" ];
          wantedBy = [ "multi-user.target" ];

          serviceConfig = {
            # Enough for a client to learn
            # its reflexive transport address (XOR-MAPPED-ADDRESS), which is all we need.
            ExecStart = [
              (lib.getExe' pkgs.stuntman "stunserver")
              "--mode"
              "basic"
              "--family"
              "4"
              "--protocol"
              "udp"
              "--primaryport"
              "3478"
              "--verbosity"
              "2"
            ];

            DynamicUser = true;
            Restart = "on-failure";

            # The daemon only needs a UDP socket.
            ProtectSystem = "strict";
            ProtectHome = true;
            PrivateTmp = true;
            PrivateDevices = true;
            NoNewPrivileges = true;
            RestrictAddressFamilies = [
              "AF_INET"
              "AF_INET6"
            ];
          };
        };

        networking.firewall.enable = false;
      };
  };
in
{
  inherit nodes vlans;
}
