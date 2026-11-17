# Homebrew formula for OpenFlex (v0.10.0).
#
# This file is the *source of truth* for the tap formula: it lives in the
# repository so the release process is reproducible, but Homebrew only sees
# formulas through a tap (or homebrew-core). Publishing is two steps:
#
#   1. Cut a release (see docs/releasing.md) and copy the SHA-256 values from
#      the release's SHA256SUMS.txt into the `sha256` fields below.
#   2. Put this file into a tap repository, e.g.
#      https://github.com/FanXingChunShui/homebrew-openflex/Formula/openflex.rb
#      and bump `version` for every release. Users then run:
#        brew install fanxingchunshui/openflex/openflex
#
# The sha256 values below are the real v0.10.0 checksums
# (https://github.com/FanXingChunShui/openflex/releases/download/v0.10.0/SHA256SUMS.txt).
# Replacing them is the only edit needed for a new release.
#
# Quick checks without a tap:
#   brew audit --strict --new ./packaging/homebrew/openflex.rb
#   brew install --build-from-source ./packaging/homebrew/openflex.rb
#   brew test openflex
class Openflex < Formula
  desc "Clean-room, cross-platform discrete-event simulation engine and CLI"
  homepage "https://github.com/FanXingChunShui/openflex"
  version "0.54.0"
  license any_of: ["MIT", "Apache-2.0"]

  # macOS releases ship a universal (arm64 + x86_64) archive.
  on_macos do
    url "https://github.com/FanXingChunShui/openflex/releases/download/v#{version}/openflex-#{version}-universal-apple-darwin.tar.gz"
    sha256 "b65bcbaf6e3b922b42e7d99f10ca976fc9a7a000e9eae6de76a62f2167c10d81"
  end

  # Linux releases are arch-specific; switch to the -musl archives if you need
  # a fully static build (for example on Alpine, where glibc is absent).
  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/FanXingChunShui/openflex/releases/download/v#{version}/openflex-#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "05963bd80c7ce76841f52aa48b4977d2971e5d6664e1b1a12ee6555355aa3b04"
    else
      url "https://github.com/FanXingChunShui/openflex/releases/download/v#{version}/openflex-#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "e5c556426c8d06b363dbe240eeb4c3fbfd3ea1f648730adbfb3c726d8a3284a9"
    end
  end

  def install
    # The archives contain a single top-level directory, e.g.
    # openflex-0.10.0-x86_64-unknown-linux-gnu/openflex.
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
