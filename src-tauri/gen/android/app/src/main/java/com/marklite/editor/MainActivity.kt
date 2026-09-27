package com.marklite.editor

import android.content.res.Configuration
import android.os.Bundle
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.activity.OnBackPressedCallback
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.core.view.ViewCompat
import com.marklite.editor.mobile.StatusBarAppearance
import com.marklite.editor.mobile.SystemInsets
import com.marklite.editor.mobile.ScreenInsets

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    updateSystemBars()
  }

  override fun onWindowFocusChanged(hasFocus: Boolean) {
    super.onWindowFocusChanged(hasFocus)
    if (hasFocus) updateSystemBars()
  }

  override fun onConfigurationChanged(newConfig: Configuration) {
    super.onConfigurationChanged(newConfig)
    updateSystemBars()
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    ViewCompat.setOnApplyWindowInsetsListener(webView) { _, insets ->
      val system = insets.getInsets(
        WindowInsetsCompat.Type.statusBars() or WindowInsetsCompat.Type.displayCutout()
      )
      val navigation = insets.getInsets(WindowInsetsCompat.Type.navigationBars())
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
      val density = resources.displayMetrics.density.toDouble()
      val css = ScreenInsets(
        system.top / density, system.right / density,
        navigation.bottom / density, system.left / density,
        ime.bottom / density
      )
      if (SystemInsets.current != css) {
        SystemInsets.current = css
        webView.evaluateJavascript(
          "window.dispatchEvent(new CustomEvent('marklite:system-insets', " +
            "{detail: {top: ${css.top}, right: ${css.right}, bottom: ${css.bottom}, left: ${css.left}, imeBottom: ${css.imeBottom}}}))",
          null
        )
      }
      insets
    }
    ViewCompat.requestApplyInsets(webView)
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        webView.evaluateJavascript("window.dispatchEvent(new Event('marklite:android-back'))", null)
      }
    })
  }

  private fun updateSystemBars() {
    WindowCompat.getInsetsController(window, window.decorView).apply {
      systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
      val systemDark = (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES
      isAppearanceLightStatusBars = !(StatusBarAppearance.dark ?: systemDark)
      hide(WindowInsetsCompat.Type.navigationBars())
      if (resources.configuration.orientation == Configuration.ORIENTATION_LANDSCAPE) {
        hide(WindowInsetsCompat.Type.statusBars())
      } else {
        show(WindowInsetsCompat.Type.statusBars())
      }
    }
  }
}
