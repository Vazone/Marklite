package com.marklite.editor.mobile

import android.app.Activity
import androidx.core.view.WindowCompat

object StatusBarAppearance {
  @Volatile var dark: Boolean? = null

  fun apply(activity: Activity) {
    val darkTheme = dark ?: return
    WindowCompat.getInsetsController(activity.window, activity.window.decorView)
      .isAppearanceLightStatusBars = !darkTheme
  }
}
