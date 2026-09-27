package com.marklite.editor.mobile

import android.app.Activity
import android.graphics.Color
import android.graphics.pdf.PdfDocument
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
import org.json.JSONArray
import org.json.JSONObject
import org.json.JSONTokener

/** A bounded native-canvas PDF batch. Each instance owns one WebView and one file. */
internal class ExportPdfSession(
  private val activity: Activity,
  private val invoke: Invoke,
  private val args: DrawPdfArgs,
  private val onFinish: () -> Unit
) {
  private val handler = Handler(Looper.getMainLooper())
  private var view: WebView? = null
  private var output: File? = null
  private var finished = false
  private var drawing = false

  fun start() {
    try {
      if (args.html.length > 6 * 1024 * 1024) throw IllegalArgumentException("PDF batch exceeds 6 MiB")
      if (args.paperSize !in listOf("a4", "letter") || args.orientation !in listOf("portrait", "landscape") ||
        args.margin !in listOf("narrow", "normal", "wide")) throw IllegalArgumentException("Invalid PDF options")
      val root = File(activity.dataDir, "export-staging").canonicalFile
      val file = File(args.stagedPath).canonicalFile
      if (!file.path.startsWith(root.path + File.separator) || file.parentFile?.parentFile != root ||
        file.exists() || file.extension != "pdf") throw SecurityException("PDF path is not a new private staging file")
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
      handler.postDelayed({ fail("PDF batch timed out") }, 45_000)
    } catch (error: Exception) { fail(error.message ?: "PDF batch could not start") }
  }

  private fun measure(webView: WebView) {
    if (finished || drawing) return
    webView.evaluateJavascript(
      "Math.ceil(Math.max(document.documentElement.scrollHeight, document.body.scrollHeight, document.querySelector('main')?.getBoundingClientRect().bottom || 0) * window.devicePixelRatio)"
    ) { raw ->
      if (finished || drawing) return@evaluateJavascript
      try {
        val height = (JSONTokener(raw).nextValue() as Number).toInt().coerceAtLeast(1)
        if (height > 8192) return@evaluateJavascript fail("PDF_BATCH_HEIGHT_LIMIT: batch exceeds mobile layout budget")
        drawing = true
        webView.measure(View.MeasureSpec.makeMeasureSpec(1200, View.MeasureSpec.EXACTLY),
          View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY))
        webView.layout(0, 0, 1200, height)
        handler.postDelayed({ collectLayout(webView, height) }, 100)
      } catch (error: Exception) { fail(error.message ?: "PDF layout is invalid") }
    }
  }

  private fun collectLayout(webView: WebView, height: Int) {
    if (finished) return
    val script = """(() => {
      const ratio = window.devicePixelRatio;
      const links = Array.from(document.querySelectorAll('a[href]:not([aria-hidden="true"])')).slice(0, 2048)
        .flatMap(node => Array.from(node.getClientRects()).map(rect => ({
          href: node.href, left: rect.left * ratio, top: rect.top * ratio,
          right: rect.right * ratio, bottom: rect.bottom * ratio
        })));
      const anchors = Array.from(document.querySelectorAll('[id]')).slice(0, 2048)
        .map(node => ({ id: node.id, top: node.getBoundingClientRect().top * ratio }));
      const breaks = Array.from(document.querySelectorAll('main > *')).slice(0, 4096)
        .map(node => node.getBoundingClientRect().top * ratio)
        .filter(value => Number.isFinite(value) && value > 0);
      return JSON.stringify({ links, anchors, breaks });
    })()"""
    webView.evaluateJavascript(script) { raw ->
      if (finished) return@evaluateJavascript
      try {
        val layout = JSONTokener(raw).nextValue() as String
        if (layout.length > 1024 * 1024) throw IllegalArgumentException("PDF link layout exceeds budget")
        draw(webView, height, layout)
      } catch (error: Exception) { fail(error.message ?: "PDF link layout is invalid") }
    }
  }

  private fun draw(webView: WebView, height: Int, layout: String) {
    if (finished) return
    try {
      var width = if (args.paperSize == "a4") 595 else 612
      var pageHeight = if (args.paperSize == "a4") 842 else 792
      if (args.orientation == "landscape") { val swap = width; width = pageHeight; pageHeight = swap }
      val margin = when (args.margin) { "narrow" -> 36; "wide" -> 108; else -> 72 }
      val contentWidth = width - 2 * margin
      val contentHeight = pageHeight - 2 * margin
      if (contentWidth <= 0 || contentHeight <= 0) throw IllegalArgumentException("PDF margins exceed page")
      val scale = contentWidth.toFloat() / 1200f
      val sourcePageHeight = contentHeight / scale
      val layoutObject = JSONObject(layout)
      val breaks = layoutObject.getJSONArray("breaks")
      val pageOffsets = mutableListOf(0f)
      while (pageOffsets.last() + sourcePageHeight < height) {
        val start = pageOffsets.last()
        val target = start + sourcePageHeight
        var next = target
        for (index in 0 until breaks.length()) {
          val candidate = breaks.getDouble(index).toFloat()
          if (candidate > start + sourcePageHeight * .5f && candidate < target) next = candidate
        }
        pageOffsets.add(next)
        if (pageOffsets.size > 16) throw IllegalArgumentException("PDF batch exceeds 16 pages")
      }
      val pageCount = pageOffsets.size
      if (pageCount > 16) throw IllegalArgumentException("PDF batch exceeds 16 pages")
      layoutObject.put("pageOffsets", JSONArray(pageOffsets))
      val pdf = PdfDocument()
      try {
        for (index in 0 until pageCount) {
          val page = pdf.startPage(PdfDocument.PageInfo.Builder(width, pageHeight, index + 1).create())
          val canvas = page.canvas
          canvas.drawColor(Color.WHITE)
          canvas.save()
          canvas.clipRect(margin.toFloat(), margin.toFloat(), (width - margin).toFloat(), (pageHeight - margin).toFloat())
          canvas.translate(margin.toFloat(), margin.toFloat())
          canvas.scale(scale, scale)
          canvas.translate(0f, -pageOffsets[index])
          webView.draw(canvas)
          canvas.restore()
          pdf.finishPage(page)
        }
        val file = output ?: throw IllegalStateException("PDF output is unavailable")
        Thread {
          try {
            FileOutputStream(file).use { stream -> pdf.writeTo(stream); stream.fd.sync() }
            val bytes = file.length()
            handler.post {
              if (finished) file.delete()
              else if (bytes !in 8..(64L * 1024 * 1024) ||
                !file.inputStream().use { input -> input.readNBytes(5).contentEquals("%PDF-".toByteArray(Charsets.US_ASCII)) }) {
                fail("PDF batch is invalid or exceeds 64 MiB")
              } else succeed(bytes, pageCount, layoutObject.toString())
            }
          } catch (error: Exception) { handler.post { fail(error.message ?: "PDF encoding failed") } }
          finally { pdf.close() }
        }.start()
      } catch (error: Exception) { pdf.close(); throw error }
    } catch (error: Exception) { fail(error.message ?: "PDF drawing failed") }
  }

  private fun succeed(bytes: Long, pages: Int, layout: String) {
    if (finished) return
    cleanup(false)
    invoke.resolve(JSObject().apply { put("bytes", bytes); put("pages", pages); put("layout", layout) })
  }

  private fun fail(message: String) {
    if (finished) return
    cleanup(true)
    invoke.reject(message)
  }

  fun cancel() { fail("PDF drawing cancelled") }

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
