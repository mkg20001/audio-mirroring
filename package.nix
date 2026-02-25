{
  lib,
  cargo,
  desktop-file-utils,
  fetchFromGitLab,
  glib,
  gtk4,
  libadwaita,
  meson,
  ninja,
  pipewire,
  pkg-config,
  rustPlatform,
  rustc,
  stdenv,
  wrapGAppsHook4,
}:

stdenv.mkDerivation rec {
  name = "audio-mirroring";

  src = ./.;

  cargoDeps = rustPlatform.importCargoLock {
    lockFile = ./Cargo.lock;
  };

  nativeBuildInputs = [
    meson
    ninja
    pkg-config
    rustPlatform.cargoSetupHook
    cargo
    rustc
    rustPlatform.bindgenHook
    wrapGAppsHook4
  ];

  buildInputs = [
    desktop-file-utils
    glib
    gtk4
    libadwaita
    pipewire
  ];

  meta = with lib; {
    description = "Audio Mirroring for Linux";
    homepage = "https://github.com/mkg20001/audio-mirroring";
    license = licenses.gpl3Only;
    maintainers = with maintainers; [ mkg20001 ];
    platforms = platforms.linux;
    mainProgram = "audio-mirroring";
  };
}
