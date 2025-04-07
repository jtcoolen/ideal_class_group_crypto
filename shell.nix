with import <nixpkgs> { };
gccStdenv.mkDerivation {
  name = "env";
  nativeBuildInputs = [ gcc ];
  buildInputs = [
    gmp
    pkg-config
    m4
    cmake
    libev
    libiconv
    autoconf
    gnumake
    cmake
    openssl
    bison
    gnumake
    pkg-config
    gmp
    gmpxx
    libcxx
    rustup
    ninja
    valgrind
  ];
 }
