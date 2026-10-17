{
  self,
  rustPlatform,
  lib,
  ...
}:
let
  inherit (self.lib) fs;

  compFileset = ./../../../..;
  lockFile = "${compFileset}/Cargo.lock";
  buildType = "debug";
  target = "main";
in
rustPlatform.buildRustPackage {
  pname = "service";
  version = "0.0.1";

  src = fs.toSource { filesets = [ compFileset ]; };

  cargoDeps = rustPlatform.importCargoLock {
    inherit lockFile;
  };

  buildType = if buildType == "debug" then "debug" else "release";
  buildFeatures = [ ];

  cargoBuildFlags = [
    "--bin"
    target
  ];

  doCheck = false;

  meta = {
    description = "service";
    homepage = "https://github.com/gabyx/nat-traversal";
    license = lib.licenses.mit;
    maintainers = [ "gabyx" ];
    mainProgram = "service";
  };
}
