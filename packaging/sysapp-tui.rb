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
  version "0.3.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/ShiGaChenTW/sysapp-tui/releases/download/v#{version}/sysapp-tui-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "807ea49a309aed54367504e28dc5f6ba8a92e68c08edaabd7f01c71674a04a85"
    else
      url "https://github.com/ShiGaChenTW/sysapp-tui/releases/download/v#{version}/sysapp-tui-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "7491f55dfbcfb6012cef57359e12eef5858ebd3644eeae48e41df616387f7868"
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
