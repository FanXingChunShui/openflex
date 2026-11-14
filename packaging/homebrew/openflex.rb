# Homebrew formula template for OpenFlex.
#
# This file is a *template*: it lives in the repository so the release process
# is reproducible, but Homebrew only sees formulas through a tap (or
# homebrew-core). Publishing is two steps:
#
#   1. Cut a release (see docs/releasing.md) and copy the SHA-256 values from
#      the release's SHA256SUMS.txt into the `sha256` fields below.
#   2. Put this file into a tap repository, e.g.
#      https://github.com/openflex/homebrew-openflex/Formula/openflex.rb
#      and bump `version` for every release. Users then run:
#        brew install openflex/openflex/openflex
#
# Placeholders to replace before publishing:
#   REPLACE_WITH_SHA256_UNIVERSAL_APPLE_DARWIN
#   REPLACE_WITH_SHA256_X86_64_UNKNOWN_LINUX_GNU
#   REPLACE_WITH_SHA256_AARCH64_UNKNOWN_LINUX_GNU
#
# Quick manual check without a tap:
#   brew install --build-from-source ./packaging/homebrew/openflex.rb
class Openflex < Formula
  desc "Clean-room, cross-platform discrete-event simulation engine and CLI"
  homepage "https://github.com/openflex/openflex"
  version "0.1.0"
  license "MIT OR Apache-2.0"

  # macOS releases ship a universal (arm64 + x86_64) archive.
  on_macos do
    url "https://github.com/openflex/openflex/releases/download/v#{version}/openflex-#{version}-universal-apple-darwin.tar.gz"
    sha256 "REPLACE_WITH_SHA256_UNIVERSAL_APPLE_DARWIN"
  end

  # Linux releases are arch-specific; switch to the -musl archives if you need
  # a fully static build (for example on Alpine, where glibc is absent).
  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/openflex/openflex/releases/download/v#{version}/openflex-#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_SHA256_AARCH64_UNKNOWN_LINUX_GNU"
    else
      url "https://github.com/openflex/openflex/releases/download/v#{version}/openflex-#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_SHA256_X86_64_UNKNOWN_LINUX_GNU"
    end
  end

  def install
    # The archives contain a single top-level directory, e.g.
    # openflex-0.1.0-x86_64-unknown-linux-gnu/openflex.
    binary = Dir["**/openflex"].find { |path| File.file?(path) && File.executable?(path) }
    odie "openflex binary not found in the archive" if binary.nil?

    bin.install binary => "openflex"

    readme = Dir["**/README.md"].find { |path| File.file?(path) }
    doc.install readme if readme
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/openflex --version")
    # A model that must validate and run without crashing.
    (testpath/"smoke").mkpath
    system bin/"openflex", "new", testpath/"smoke", "--template", "mm1"
    system bin/"openflex", "validate", testpath/"smoke/mm1.ofm"
    assert_match "Sink", shell_output("#{bin}/openflex run #{testpath}/smoke/mm1.ofm --format csv")
  end
end
