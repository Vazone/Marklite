package com.marklite.editor.mobile

data class ScreenInsets(
  val top: Double, val right: Double, val bottom: Double, val left: Double,
  val imeBottom: Double
)

/** CSS pixel distances occupied by visible system bars, the cutout, and the IME. */
object SystemInsets {
  @Volatile var current = ScreenInsets(0.0, 0.0, 0.0, 0.0, 0.0)
}
