# Homebrew formula for sysapp-tui.
#
# Lives here as the source of truth; the copy that users install from belongs
# in the tap repository (ShiGaChenTW/homebrew-tap) as Formula/sysapp-tui.rb.
#
# Release procedure:
#   1. bump `version` in Cargo.toml, commit
#   2. git tag vX.Y.Z && git push --tags   → the release workflow builds both
#      macOS targets and publishes the tarballs plus their .sha256 files
#   3. copy the two checksums from the workflow's "Show checksums" step into
#      the `sha256` lines below
#   4. copy this file into the tap repo and push
#
class SysappTui < Formula
  desc "macOS system package scanner and TUI dashboard"
  homepage "https://github.com/ShiGaChenTW/sysapp-tui"
  version "0.2.1"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/ShiGaChenTW/sysapp-tui/releases/download/v#{version}/sysapp-tui-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "d81ccd7380263deee4a4c419cf1091d8381b6b3487db55dd22a3dfdc46f07df9"
    else
      url "https://github.com/ShiGaChenTW/sysapp-tui/releases/download/v#{version}/sysapp-tui-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "27ab6a92e0a7824d212c184fb2e3b9e9b10ec3d8c15d5e5936ad933e67f8ef04"
    end
  end

  def install
    bin.install "sysapp-tui"
  end

  test do
    # The TUI needs a tty, so the smoke test exercises the non-interactive
    # paths only — enough to prove the binary runs and is the expected build.
    assert_match version.to_s, shell_output("#{bin}/sysapp-tui --version")
    assert_match "sysapp-tui", shell_output("#{bin}/sysapp-tui --help")
  end
end
