package com.marklite.editor.mobile

import android.app.Activity
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import android.webkit.WebViewClient
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import org.json.JSONTokener

/** An attached offscreen WebView with one bounded render request and one owner. */
internal class ExportRenderSession(
  private val activity: Activity,
  private val invoke: Invoke,
  private val html: String,
  private val onFinish: () -> Unit
) {
  private val handler = Handler(Looper.getMainLooper())
  private val started = SystemClock.elapsedRealtime()
  private var webView: WebView? = null
  private var finished = false

  fun start() {
    if (html.length > 6 * 1024 * 1024) {
      fail("Diagram render page exceeds 6 MiB")
      return
    }
    try {
      val root = activity.window.decorView as ViewGroup
      val view = WebView(activity)
      webView = view
      view.settings.javaScriptEnabled = true
      view.settings.domStorageEnabled = false
      view.settings.allowFileAccess = false
      view.settings.allowContentAccess = false
      view.settings.blockNetworkLoads = true
      view.isClickable = false
      view.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
      view.translationX = 2000f
      view.webViewClient = object : WebViewClient() {
        override fun onPageFinished(view: WebView, url: String) { poll() }
      }
      root.addView(view, ViewGroup.LayoutParams(1024, 768))
      view.loadDataWithBaseURL("https://marklite.invalid/", html, "text/html", "UTF-8", null)
      handler.postDelayed({ fail("Diagram render timed out") }, 30_000)
    } catch (error: Exception) {
      fail(error.message ?: "Diagram render could not start")
    }
  }

  private fun poll() {
    if (finished) return
    if (SystemClock.elapsedRealtime() - started >= 30_000) {
      fail("Diagram render timed out")
      return
    }
    val view = webView ?: return fail("Diagram WebView was released")
    view.evaluateJavascript("window.__markliteDiagramExport?.status || 'pending'") { value ->
      if (finished) return@evaluateJavascript
      val status = try { JSONTokener(value).nextValue() as String }
        catch (_: Exception) { return@evaluateJavascript fail("Diagram status is invalid") }
      when (status) {
        "pending" -> handler.postDelayed({ poll() }, 50)
        "ready", "failed" -> view.evaluateJavascript(
          "JSON.stringify(window.__markliteDiagramExport)"
        ) { result ->
          if (finished) return@evaluateJavascript
          if (result.length > 32 * 1024 * 1024) fail("Diagram result exceeds mobile budget")
          else succeed(result)
        }
        else -> fail("Diagram status is invalid")
      }
    }
  }

  private fun succeed(result: String) {
    if (finished) return
    cleanup()
    invoke.resolve(JSObject().apply { put("result", result) })
  }

  private fun fail(message: String) {
    if (finished) return
    cleanup()
    invoke.reject(message)
  }

  fun cancel() { fail("Diagram render cancelled") }

  private fun cleanup() {
    finished = true
    handler.removeCallbacksAndMessages(null)
    webView?.let { view ->
      (view.parent as? ViewGroup)?.removeView(view)
      view.stopLoading()
      view.destroy()
    }
    webView = null
    onFinish()
  }
}
