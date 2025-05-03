> [!NOTE]
> My main motivation at this point for this project is for me to learn about
> and play around with Rust on (tiny) embedded systems and to tinker with
> building an emulation framework for Cortex-M based devices. I have no idea
> where this journey will ultimately take me...

# Nano Loader
[![Rust](https://img.shields.io/badge/Rust-%23000000.svg?e&logo=rust&logoColor=white)](#)

<p align="center">
<img width="250" src="nanoloader/src/.doc/nanoloader.png">
</p>

_Nano Loader_ is an update-capable bootloader inspired by [Basic
Loader](https://github.com/mkuyper/basicloader).

Nano Loader is intended to be portable to any Cortex-M based device, but the
first target platform for Nano Loader is TI's
[MSPM0C1104](https://www.ti.com/product/MSPM0C1104).
