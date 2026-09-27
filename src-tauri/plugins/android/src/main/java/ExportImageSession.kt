package com.marklite.editor.mobile

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.Handler
import android.os.Looper
import android.view.View
import android.view.ViewGroup
import android.webkit.WebView
import android.webkit.WebViewClient
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import java.io.File
import java.io.FileOutputStream
import org.json.JSONTokener

/** Captures exactly one chapter, bounded before bitmap allocation. */
internal class ExportImageSession(
  private val activity: Activity,
  private val invoke: Invoke,
  private val args: CapturePngArgs,
  private val onFinish: () -> Unit
) {
  private val handler = Handler(Looper.getMainLooper())
  private var view: WebView? = null
  private var output: File? = null
  private var finished = false
  private var capturing = false

  fun start() {
    try {
      if (args.html.length > 6 * 1024 * 1024) throw IllegalArgumentException("Image chapter exceeds 6 MiB")
      val root = File(activity.dataDir, "export-staging").canonicalFile
      val file = File(args.stagedPath).canonicalFile
      if (!file.path.startsWith(root.path + File.separator) || file.parentFile?.parentFile != root ||
        file.exists() || file.extension != "png") throw SecurityException("Image path is not a new private staging file")
      output = file
      val webView = WebView(activity)
      view = webView
      webView.settings.javaScriptEnabled = true
      webView.settings.allowContentAccess = false
      webView.settings.allowFileAccess = false
      webView.settings.blockNetworkLoads = true
      webView.isClickable = false
      webView.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
      webView.translationX = 2000f
      webView.webViewClient = object : WebViewClient() {
        override fun onPageFinished(view: WebView, url: String) { measure(view) }
      }
      (activity.window.decorView as ViewGroup).addView(webView, ViewGroup.LayoutParams(1200, 768))
      webView.loadDataWithBaseURL("https://marklite.invalid/", args.html, "text/html", "UTF-8", null)
      handler.postDelayed({ fail("Image capture timed out") }, 30_000)
    } catch (error: Exception) { fail(error.message ?: "Image capture could not start") }
  }

  private fun measure(webView: WebView) {
    if (finished || capturing) return
    webView.evaluateJavascript(
      "Math.ceil(Math.max(document.documentElement.scrollHeight, document.body.scrollHeight, document.querySelector('main')?.getBoundingClientRect().bottom || 0) * window.devicePixelRatio)"
    ) { raw ->
      if (finished || capturing) return@evaluateJavascript
      try {
        val height = (JSONTokener(raw).nextValue() as Number).toInt().coerceAtLeast(1)
        if (height > 8192 || 1200L * height > 12_000_000L) {
          return@evaluateJavascript fail("Chapter exceeds mobile image height budget")
        }
        capturing = true
        val widthSpec = View.MeasureSpec.makeMeasureSpec(1200, View.MeasureSpec.EXACTLY)
        val heightSpec = View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY)
        webView.measure(widthSpec, heightSpec)
        webView.layout(0, 0, 1200, height)
        handler.postDelayed({ draw(webView, height) }, 100)
      } catch (error: Exception) { fail(error.message ?: "Image layout is invalid") }
    }
  }

  private fun draw(webView: WebView, height: Int) {
    if (finished) return
    try {
      val bitmap = Bitmap.createBitmap(1200, height, Bitmap.Config.ARGB_8888)
      val canvas = Canvas(bitmap)
      canvas.drawColor(android.graphics.Color.WHITE)
      webView.draw(canvas)
      val file = output ?: throw IllegalStateException("Image output is unavailable")
      Thread {
        try {
          FileOutputStream(file).use { stream ->
            if (!bitmap.compress(Bitmap.CompressFormat.PNG, 100, stream)) throw IllegalStateException("PNG encoding failed")
            stream.fd.sync()
          }
          val bytes = file.length()
          handler.post {
            if (finished) file.delete()
            else if (bytes !in 1..(64L * 1024 * 1024)) fail("PNG exceeds 64 MiB")
            else succeed(bytes, height)
          }
        } catch (error: Exception) {
          handler.post { fail(error.message ?: "PNG encoding failed") }
        } finally { bitmap.recycle() }
      }.start()
    } catch (error: Exception) { fail(error.message ?: "Image capture failed") }
  }

  private fun succeed(bytes: Long, height: Int) {
    if (finished) return
    cleanup(false)
    invoke.resolve(JSObject().apply { put("bytes", bytes); put("width", 1200); put("height", height) })
  }

  private fun fail(message: String) {
    if (finished) return
    cleanup(true)
    invoke.reject(message)
  }

  fun cancel() { fail("Image capture cancelled") }

  private fun cleanup(deleteOutput: Boolean) {
    finished = true
    handler.removeCallbacksAndMessages(null)
    view?.let { webView ->
      (webView.parent as? ViewGroup)?.removeView(webView)
      webView.stopLoading()
      webView.destroy()
    }
    view = null
    if (deleteOutput) output?.delete()
    onFinish()
  }
}
