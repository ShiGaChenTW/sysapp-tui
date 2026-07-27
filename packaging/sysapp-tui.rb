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
  version "0.2.2"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/ShiGaChenTW/sysapp-tui/releases/download/v#{version}/sysapp-tui-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "88024b9f7ba386bfa105067506c575a8462eea63d0e6183a0f272bda316ecfc0"
    else
      url "https://github.com/ShiGaChenTW/sysapp-tui/releases/download/v#{version}/sysapp-tui-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "f9f4363f060b3fe8bb33b56aaecf118543c81b404848c79c84e61ddf188a7dc2"
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
