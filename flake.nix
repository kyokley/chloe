{
  description = "Chloe terminal-based task management application";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-26.05-darwin";

  outputs = { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          chloe = pkgs.rustPlatform.buildRustPackage {
            pname = "chloe";
            version = "0.4.0";
            src = self;
            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ];
          };
        in
        {
          default = chloe;
          inherit chloe;
        });

      apps = forAllSystems (system:
        let
          package = self.packages.${system}.chloe;
        in
        {
          default = {
            type = "app";
            program = "${package}/bin/chloe";
          };
          chloe = self.apps.${system}.default;
        });
    };
}
