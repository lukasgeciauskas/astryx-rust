// Prints the window number of a process's first on-screen window, for
// `screencapture -l`. Used by tools/ci-macos.sh.
//
// usage: mac-window-id <pid>

import CoreGraphics
import Foundation

guard CommandLine.arguments.count == 2, let pid = Int32(CommandLine.arguments[1]) else {
    FileHandle.standardError.write("usage: mac-window-id <pid>\n".data(using: .utf8)!)
    exit(2)
}
let options: CGWindowListOption = [.optionOnScreenOnly, .excludeDesktopElements]
let windows = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] ?? []
for window in windows {
    guard let owner = window[kCGWindowOwnerPID as String] as? Int32, owner == pid,
        let layer = window[kCGWindowLayer as String] as? Int, layer == 0,
        let number = window[kCGWindowNumber as String] as? Int
    else { continue }
    print(number)
    exit(0)
}
exit(1)
