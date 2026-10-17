# Dev shell for NixOS: Rust toolchain plus what the `sdl3` crate needs to build SDL3 from source.
{ pkgs ? import <nixpkgs> { } }:
pkgs.mkShell {
  packages = with pkgs; [
    cargo
    rustc
    clippy
    rustfmt
    cmake
    pkg-config
  ];
  buildInputs = with pkgs; [
    alsa-lib
    libpulseaudio
    libGL
    libxkbcommon
    wayland
    libdecor
    libx11
    libxcb
    libxext
    libxcursor
    libxi
    libxrandr
    libxfixes
    libxscrnsaver
    libxtst
  ];
  # SDL dlopens audio/video backends at runtime.
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
    alsa-lib
    libpulseaudio
    libGL
    libxkbcommon
    wayland
    libdecor
    libx11
    libxcb
    libxext
    libxcursor
    libxi
    libxrandr
    libxfixes
    libxscrnsaver
  ]);
}
