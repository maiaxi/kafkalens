import SwiftUI

/// `SwiftUI.State` used as a plain property wrapper.
///
/// Since the macOS 27 SDK, `@State` resolves to a macro whose plugin
/// (SwiftUIMacros) ships only with Xcode, so `swift build` with just the
/// Command Line Tools fails on it. The underlying property wrapper behaves
/// the same; this alias lets the package build either way.
typealias ViewState<Value> = SwiftUI.State<Value>
