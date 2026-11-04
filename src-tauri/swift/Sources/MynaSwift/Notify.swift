// Update notifications through UNUserNotificationCenter, with clicks handed back to Rust.

import Foundation
import UserNotifications

/// UNUserNotificationCenter throws outside an app bundle, so an unbundled `tauri dev` binary sends nothing.
private func bundled() -> Bool {
    Bundle.main.bundleIdentifier != nil && Bundle.main.bundleURL.pathExtension == "app"
}

private final class Delegate: NSObject, UNUserNotificationCenterDelegate {
    let clicked: @convention(c) () -> Void
    init(_ clicked: @escaping @convention(c) () -> Void) { self.clicked = clicked }

    func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse, withCompletionHandler done: @escaping () -> Void) {
        if response.actionIdentifier == UNNotificationDefaultActionIdentifier { clicked() }
        done()
    }

    // Shown even while a Myna window has focus; macOS hides a frontmost app's notifications by default.
    func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification, withCompletionHandler done: @escaping (UNNotificationPresentationOptions) -> Void) {
        done([.banner, .list])
    }
}

private var delegate: Delegate?

/// Call once at launch: `clicked` runs when the user clicks one of Myna's notifications.
@_cdecl("myna_notify_setup")
public func notifySetup(_ clicked: @escaping @convention(c) () -> Void) {
    guard bundled() else { return }
    delegate = Delegate(clicked)
    UNUserNotificationCenter.current().delegate = delegate
}

/// Asks for permission the first time; once denied, sends nothing.
@_cdecl("myna_notify")
public func notify(_ title: UnsafePointer<CChar>, _ body: UnsafePointer<CChar>) {
    guard bundled() else { return }
    let content = UNMutableNotificationContent()
    content.title = String(cString: title)
    content.body = String(cString: body)
    let center = UNUserNotificationCenter.current()
    center.requestAuthorization(options: [.alert, .sound]) { granted, _ in
        guard granted else { return }
        center.add(UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil))
    }
}
