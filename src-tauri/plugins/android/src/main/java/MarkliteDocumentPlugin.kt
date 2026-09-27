package com.marklite.editor.mobile

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import android.util.Base64
import android.webkit.WebView
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import java.security.MessageDigest
import org.json.JSONArray
import org.json.JSONObject

private const val MAX_DOCUMENT_BYTES = 10 * 1024 * 1024
private const val MAX_EXPORT_BYTES = 64L * 1024 * 1024
private const val MAX_PDF_EXPORT_BYTES = 512L * 1024 * 1024
private const val MAX_PENDING_INTENTS = 50
private const val MAX_TREE_CHILDREN = 4096
private const val GRANTS = Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION

@InvokeArg
class DocumentUriArgs {
  var uri: String = ""
}

@InvokeArg
class CreateDocumentArgs {
  var title: String = "Untitled.md"
}

@InvokeArg
class CreateExportDocumentArgs {
  var title: String = "Untitled.svg"
  var mime: String = "image/svg+xml"
}

@InvokeArg
class TreePathArgs {
  var treeUri: String = ""
  var relativePath: String = ""
}

@InvokeArg
class TreeSourceArgs {
  var treeUri: String = ""
  var sourceUri: String = ""
}

private data class TreeChild(val id: String, val name: String, val mime: String, val size: Long?)

@InvokeArg
class DocumentWriteArgs {
  var uri: String = ""
  var content: String = ""
  var expectedVersion: String? = null
}

@InvokeArg
class ExportFileArgs {
  var uri: String = ""
  var stagedPath: String = ""
  var bytes: Long = 0
  var sha256: String = ""
}

@InvokeArg
class RenderExportArgs {
  var html: String = ""
}

@InvokeArg
class CapturePngArgs {
  var html: String = ""
  var stagedPath: String = ""
}

@InvokeArg
class DrawPdfArgs {
  var html: String = ""
  var stagedPath: String = ""
  var paperSize: String = "a4"
  var orientation: String = "portrait"
  var margin: String = "normal"
}

@InvokeArg
class BeginImageExportArgs {
  var treeUri: String = ""
  var folderName: String = ""
}

@InvokeArg
class ImageFileArgs {
  var folderUri: String = ""
  var name: String = ""
}

@InvokeArg
class ImageFolderArgs {
  var folderUri: String = ""
}

@InvokeArg
class StatusBarAppearanceArgs {
  var dark: Boolean = false
}


@TauriPlugin
class MarkliteDocumentPlugin(private val activity: Activity) : Plugin(activity) {
  private val pendingIntents = ArrayDeque<String>()
  private val pendingExports = HashSet<String>()
  private val imageFolders = HashMap<String, MutableSet<String>>()
  private var activeRender: ExportRenderSession? = null
  private var activeCapture: ExportImageSession? = null
  private var activePdf: ExportPdfSession? = null
  private var lastIntent: Intent? = null

  @Command
  fun setStatusBarAppearance(invoke: Invoke) {
    val args = invoke.parseArgs(StatusBarAppearanceArgs::class.java)
    activity.runOnUiThread {
      StatusBarAppearance.dark = args.dark
      StatusBarAppearance.apply(activity)
      invoke.resolve()
    }
  }

  @Command
  fun systemInsets(invoke: Invoke) {
    val insets = SystemInsets.current
    invoke.resolve(JSObject().apply {
      put("top", insets.top)
      put("right", insets.right)
      put("bottom", insets.bottom)
      put("left", insets.left)
      put("imeBottom", insets.imeBottom)
    })
  }

  override fun load(webView: WebView) {
    // The editor owns font sizing. WebView page scaling moves fixed controls off screen.
    webView.settings.setSupportZoom(false)
    webView.settings.builtInZoomControls = false
    captureIntent(activity.intent)
  }

  override fun onResume() {
    captureIntent(activity.intent)
  }

  override fun onPause() {
    activeRender?.cancel()
    activeCapture?.cancel()
    activePdf?.cancel()
  }

  override fun onNewIntent(intent: Intent) {
    activity.intent = intent
    captureIntent(intent)
  }

  @Command
  fun pickDocument(invoke: Invoke) {
    val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
      addCategory(Intent.CATEGORY_OPENABLE)
      type = "*/*"
      putExtra(Intent.EXTRA_MIME_TYPES, arrayOf("text/plain", "text/markdown", "text/x-markdown", "application/octet-stream"))
      addFlags(GRANTS or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
    }
    startActivityForResult(invoke, intent, "documentPicked")
  }

  @Command
  fun createDocument(invoke: Invoke) {
    val args = invoke.parseArgs(CreateDocumentArgs::class.java)
    val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
      addCategory(Intent.CATEGORY_OPENABLE)
      type = "text/markdown"
      putExtra(Intent.EXTRA_TITLE, args.title)
      addFlags(GRANTS or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
    }
    startActivityForResult(invoke, intent, "documentPicked")
  }

  @Command
  fun createExportDocument(invoke: Invoke) {
    val args = invoke.parseArgs(CreateExportDocumentArgs::class.java)
    val allowed = mapOf(
      "html" to "text/html",
      "pdf" to "application/pdf",
      "docx" to "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
      "svg" to "image/svg+xml"
    )
    val extension = args.title.substringAfterLast('.', "").lowercase()
    if (!validSegment(args.title) || allowed[extension] != args.mime) {
      invoke.reject("Invalid export document name or MIME type")
      return
    }
    val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
      addCategory(Intent.CATEGORY_OPENABLE)
      type = args.mime
      putExtra(Intent.EXTRA_TITLE, args.title)
      addFlags(GRANTS or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
    }
    startActivityForResult(invoke, intent, "exportDocumentPicked")
  }

  @Command
  fun renderExportDiagrams(invoke: Invoke) {
    val args = invoke.parseArgs(RenderExportArgs::class.java)
    activity.runOnUiThread {
      if (activeRender != null) {
        invoke.reject("A diagram export renderer is already running")
        return@runOnUiThread
      }
      val session = ExportRenderSession(activity, invoke, args.html) { activeRender = null }
      activeRender = session
      session.start()
    }
  }

  @Command
  fun captureExportPng(invoke: Invoke) {
    val args = invoke.parseArgs(CapturePngArgs::class.java)
    activity.runOnUiThread {
      if (activeCapture != null) {
        invoke.reject("An image export renderer is already running")
        return@runOnUiThread
      }
      val session = ExportImageSession(activity, invoke, args) { activeCapture = null }
      activeCapture = session
      session.start()
    }
  }

  @Command
  fun cancelCapturePng(invoke: Invoke) {
    activity.runOnUiThread {
      activeCapture?.cancel()
      invoke.resolve()
    }
  }

  @Command
  fun drawExportPdf(invoke: Invoke) {
    val args = invoke.parseArgs(DrawPdfArgs::class.java)
    activity.runOnUiThread {
      if (activePdf != null) {
        invoke.reject("A PDF export renderer is already running")
        return@runOnUiThread
      }
      val session = ExportPdfSession(activity, invoke, args) { activePdf = null }
      activePdf = session
      session.start()
    }
  }

  @Command
  fun cancelDrawPdf(invoke: Invoke) {
    activity.runOnUiThread {
      activePdf?.cancel()
      invoke.resolve()
    }
  }

  @Command
  fun beginImageExport(invoke: Invoke) {
    val args = invoke.parseArgs(BeginImageExportArgs::class.java)
    Thread {
      try {
        val tree = requireContentUri(args.treeUri)
        if (!validSegment(args.folderName)) throw IllegalArgumentException("Invalid image folder name")
        val rootId = DocumentsContract.getTreeDocumentId(tree)
        if (children(tree, rootId).any { it.name == args.folderName }) {
          throw IllegalStateException("PNG_TARGET_EXISTS")
        }
        val parent = DocumentsContract.buildDocumentUriUsingTree(tree, rootId)
        val folder = DocumentsContract.createDocument(activity.contentResolver, parent,
          DocumentsContract.Document.MIME_TYPE_DIR, args.folderName)
          ?: throw IllegalStateException("Provider did not create image folder")
        if (displayName(folder) != args.folderName) {
          DocumentsContract.deleteDocument(activity.contentResolver, folder)
          throw IllegalStateException("Provider changed image folder name")
        }
        synchronized(imageFolders) { imageFolders[folder.toString()] = HashSet<String>() }
        invoke.resolve(JSObject().apply { put("uri", folder.toString()) })
      } catch (error: Exception) { invoke.reject(error.message ?: "Image folder creation failed") }
    }.start()
  }

  @Command
  fun createImageFile(invoke: Invoke) {
    val args = invoke.parseArgs(ImageFileArgs::class.java)
    Thread {
      try {
        val folder = requireContentUri(args.folderUri)
        if (!validSegment(args.name) || !args.name.endsWith(".png", ignoreCase = true)) {
          throw IllegalArgumentException("Invalid chapter image name")
        }
        synchronized(imageFolders) {
          if (!imageFolders.containsKey(args.folderUri)) throw SecurityException("Image folder is not owned by this session")
        }
        val image = DocumentsContract.createDocument(activity.contentResolver, folder, "image/png", args.name)
          ?: throw IllegalStateException("Provider did not create chapter image")
        if (displayName(image) != args.name) {
          DocumentsContract.deleteDocument(activity.contentResolver, image)
          throw IllegalStateException("Provider changed chapter image name")
        }
        synchronized(imageFolders) { imageFolders[args.folderUri]?.add(image.toString()) }
        synchronized(pendingExports) { pendingExports.add(image.toString()) }
        invoke.resolve(JSObject().apply { put("uri", image.toString()) })
      } catch (error: Exception) { invoke.reject(error.message ?: "Chapter image creation failed") }
    }.start()
  }

  @Command
  fun finishImageExport(invoke: Invoke) {
    val args = invoke.parseArgs(ImageFolderArgs::class.java)
    Thread {
      try {
        val children = synchronized(imageFolders) { imageFolders[args.folderUri]?.toList() }
          ?: throw SecurityException("Image folder is not owned by this session")
        if (children.isEmpty() || synchronized(pendingExports) { children.any { pendingExports.contains(it) } }) {
          throw IllegalStateException("Image export still has pending files")
        }
        synchronized(imageFolders) { imageFolders.remove(args.folderUri) }
        invoke.resolve()
      } catch (error: Exception) { invoke.reject(error.message ?: "Image export could not finish") }
    }.start()
  }

  @Command
  fun abortImageExport(invoke: Invoke) {
    val args = invoke.parseArgs(ImageFolderArgs::class.java)
    Thread {
      try {
        val children = synchronized(imageFolders) { imageFolders.remove(args.folderUri) }
        if (children != null) {
          synchronized(pendingExports) { pendingExports.removeAll(children) }
          DocumentsContract.deleteDocument(activity.contentResolver, requireContentUri(args.folderUri))
        }
        invoke.resolve()
      } catch (error: Exception) { invoke.reject(error.message ?: "Image export cleanup failed") }
    }.start()
  }


  @Command
  fun pickTree(invoke: Invoke) {
    val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
      addFlags(GRANTS or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
    }
    startActivityForResult(invoke, intent, "treePicked")
  }

  @Command
  fun inspectTree(invoke: Invoke) {
    val args = invoke.parseArgs(TreePathArgs::class.java)
    Thread {
      try {
        val tree = requireContentUri(args.treeUri)
        val rootId = DocumentsContract.getTreeDocumentId(tree)
        val root = DocumentsContract.buildDocumentUriUsingTree(tree, rootId)
        invoke.resolve(JSObject().apply { put("name", displayName(root)) })
      } catch (error: Exception) {
        invoke.reject(error.message ?: "Tree inspection failed")
      }
    }.start()
  }

  @Command
  fun listTree(invoke: Invoke) {
    val args = invoke.parseArgs(TreePathArgs::class.java)
    Thread {
      try {
        val tree = requireContentUri(args.treeUri)
        val parent = directoryId(tree, args.relativePath)
        val rows = JSONArray()
        for (child in children(tree, parent)) {
          if (!validSegment(child.name)) continue
          val directory = child.mime == DocumentsContract.Document.MIME_TYPE_DIR
          if (!directory && !isMarkdown(child.name)) continue
          val childUri = DocumentsContract.buildDocumentUriUsingTree(tree, child.id)
          rows.put(JSONObject().apply {
            put("name", child.name)
            put("uri", childUri.toString())
            put("kind", if (directory) "directory" else "markdown")
            put("size", if (directory || child.size == null) JSONObject.NULL else child.size)
          })
        }
        invoke.resolve(JSObject().apply { put("entries", rows) })
      } catch (error: Exception) {
        invoke.reject(error.message ?: "Tree listing failed")
      }
    }.start()
  }

  @Command
  fun resolveTreeFile(invoke: Invoke) {
    val args = invoke.parseArgs(TreePathArgs::class.java)
    Thread {
      try {
        val tree = requireContentUri(args.treeUri)
        val parts = pathParts(args.relativePath)
        if (parts.isEmpty()) throw IllegalArgumentException("A document path is required")
        val parent = directoryId(tree, parts.dropLast(1).joinToString("/"))
        val matches = children(tree, parent).filter { it.name == parts.last() }
        if (matches.size != 1 || matches[0].mime == DocumentsContract.Document.MIME_TYPE_DIR ||
          !isMarkdown(matches[0].name)) throw IllegalArgumentException("Markdown document is unavailable")
        val uri = DocumentsContract.buildDocumentUriUsingTree(tree, matches[0].id)
        invoke.resolve(JSObject().apply { put("uri", uri.toString()) })
      } catch (error: Exception) {
        invoke.reject(error.message ?: "Tree document resolution failed")
      }
    }.start()
  }

  @Command
  fun resolveTreeImage(invoke: Invoke) {
    val args = invoke.parseArgs(TreePathArgs::class.java)
    Thread {
      try {
        val tree = requireContentUri(args.treeUri)
        val parts = pathParts(args.relativePath)
        if (parts.isEmpty() || !isImage(parts.last())) throw IllegalArgumentException("Image path is invalid")
        val parent = directoryId(tree, parts.dropLast(1).joinToString("/"))
        val matches = children(tree, parent).filter { it.name == parts.last() &&
          it.mime != DocumentsContract.Document.MIME_TYPE_DIR }
        if (matches.size != 1) throw IllegalArgumentException("Image is unavailable or ambiguous")
        val uri = DocumentsContract.buildDocumentUriUsingTree(tree, matches[0].id)
        invoke.resolve(JSObject().apply { put("uri", uri.toString()) })
      } catch (error: Exception) {
        invoke.reject(error.message ?: "Tree image resolution failed")
      }
    }.start()
  }

  @Command
  fun locateTreeDocument(invoke: Invoke) {
    val args = invoke.parseArgs(TreeSourceArgs::class.java)
    Thread {
      try {
        val tree = requireContentUri(args.treeUri)
        val source = requireContentUri(args.sourceUri)
        val relative = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && tree.authority == source.authority) {
          val sourceId = DocumentsContract.getDocumentId(source)
          val treeDocument = DocumentsContract.buildDocumentUriUsingTree(tree, sourceId)
          val ids = DocumentsContract.findDocumentPath(activity.contentResolver, treeDocument)?.path
          val rootId = DocumentsContract.getTreeDocumentId(tree)
          if (ids != null && ids.size in 2..65 && ids.first() == rootId && ids.last() == sourceId) {
            val names = ids.drop(1).map { id ->
              val uri = DocumentsContract.buildDocumentUriUsingTree(tree, id)
              queriedName(uri)
            }
            if (names.all { it != null && validSegment(it) }) names.filterNotNull().joinToString("/") else null
          } else null
        } else null
        invoke.resolve(JSObject().apply { put("relativePath", relative ?: JSONObject.NULL) })
      } catch (error: UnsupportedOperationException) {
        invoke.resolve(JSObject().apply { put("relativePath", JSONObject.NULL) })
      } catch (error: Exception) {
        invoke.reject(error.message ?: "Tree document lookup failed")
      }
    }.start()
  }

  @Command
  fun readImage(invoke: Invoke) {
    val args = invoke.parseArgs(DocumentUriArgs::class.java)
    Thread {
      try {
        val uri = requireContentUri(args.uri)
        val bytes = readBytes(uri)
        invoke.resolve(JSObject().apply { put("base64", Base64.encodeToString(bytes, Base64.NO_WRAP)) })
      } catch (error: Exception) {
        invoke.reject(error.message ?: "Image read failed")
      }
    }.start()
  }

  @ActivityCallback
  fun documentPicked(invoke: Invoke, result: ActivityResult) {
    finishPick(invoke, result)
  }

  @ActivityCallback
  fun exportDocumentPicked(invoke: Invoke, result: ActivityResult) {
    finishPick(invoke, result, true)
  }

  @ActivityCallback
  fun treePicked(invoke: Invoke, result: ActivityResult) {
    finishPick(invoke, result)
  }

  private fun finishPick(invoke: Invoke, result: ActivityResult, export: Boolean = false) {
    if (result.resultCode != Activity.RESULT_OK) {
      invoke.resolve(JSObject().apply { put("uri", JSONObject.NULL) })
      return
    }
    try {
      val uri = requireContentUri(result.data?.data?.toString() ?: "")
      val flags = result.data?.flags ?: 0
      val granted = flags and GRANTS
      if (granted and Intent.FLAG_GRANT_READ_URI_PERMISSION == 0) {
        throw SecurityException("The provider did not grant read access")
      }
      activity.contentResolver.takePersistableUriPermission(uri, granted)
      if (export) synchronized(pendingExports) { pendingExports.add(uri.toString()) }
      invoke.resolve(JSObject().apply { put("uri", uri.toString()) })
    } catch (error: Exception) {
      invoke.reject(error.message ?: "Document selection failed")
    }
  }

  @Command
  fun readDocument(invoke: Invoke) {
    val args = invoke.parseArgs(DocumentUriArgs::class.java)
    Thread {
      try {
        val uri = requireContentUri(args.uri)
        val bytes = readBytes(uri)
        val content = Charsets.UTF_8.newDecoder()
          .onMalformedInput(CodingErrorAction.REPORT)
          .onUnmappableCharacter(CodingErrorAction.REPORT)
          .decode(ByteBuffer.wrap(bytes)).toString()
        invoke.resolve(JSObject().apply {
          put("uri", uri.toString())
          put("name", displayName(uri))
          put("content", content)
          put("version", digest(bytes))
          put("size", bytes.size)
        })
      } catch (error: Exception) {
        invoke.reject(error.message ?: "Document read failed")
      }
    }.start()
  }

  @Command
  fun documentName(invoke: Invoke) {
    val args = invoke.parseArgs(DocumentUriArgs::class.java)
    Thread {
      try {
        val uri = requireContentUri(args.uri)
        invoke.resolve(JSObject().apply { put("name", queriedName(uri)) })
      } catch (error: Exception) {
        invoke.reject(error.message ?: "Document name lookup failed")
      }
    }.start()
  }

  @Command
  fun documentVersion(invoke: Invoke) {
    val args = invoke.parseArgs(DocumentUriArgs::class.java)
    Thread {
      try {
        val uri = requireContentUri(args.uri)
        invoke.resolve(JSObject().apply { put("version", digest(readBytes(uri))) })
      } catch (error: Exception) {
        invoke.reject(error.message ?: "Document version check failed")
      }
    }.start()
  }

  @Command
  fun writeDocument(invoke: Invoke) {
    val args = invoke.parseArgs(DocumentWriteArgs::class.java)
    Thread {
      try {
        val uri = requireContentUri(args.uri)
        val bytes = args.content.toByteArray(Charsets.UTF_8)
        if (bytes.size > MAX_DOCUMENT_BYTES) throw IllegalArgumentException("Document exceeds 10 MiB")
        if (args.expectedVersion != null && digest(readBytes(uri)) != args.expectedVersion) {
          throw IllegalStateException("FILE_CONTENT_CHANGED")
        }
        val stream = activity.contentResolver.openOutputStream(uri, "wt")
          ?: throw IllegalStateException("Provider did not open a write stream")
        stream.use { it.write(bytes); it.flush() }
        val committed = readBytes(uri)
        if (!committed.contentEquals(bytes)) throw IllegalStateException("Provider returned different content after write")
        invoke.resolve(JSObject().apply {
          put("uri", uri.toString())
          put("name", displayName(uri))
          put("version", digest(committed))
          put("size", committed.size)
        })
      } catch (error: Exception) {
        invoke.reject(error.message ?: "Document write failed")
      }
    }.start()
  }

  @Command
  fun commitExportFile(invoke: Invoke) {
    val args = invoke.parseArgs(ExportFileArgs::class.java)
    Thread {
      try {
        val uri = requireContentUri(args.uri)
        if (!synchronized(pendingExports) { pendingExports.contains(args.uri) }) {
          throw SecurityException("Export target was not created by this session")
        }
        val root = File(activity.dataDir, "export-staging").canonicalFile
        val staged = File(args.stagedPath).canonicalFile
        if (!staged.path.startsWith(root.path + File.separator) || !staged.isFile) {
          throw SecurityException("Export staging file is outside the private directory")
        }
        val maxBytes = if (staged.extension == "pdf") MAX_PDF_EXPORT_BYTES else MAX_EXPORT_BYTES
        if (args.bytes !in 1..maxBytes || !args.sha256.matches(Regex("[0-9a-f]{64}"))) {
          throw IllegalArgumentException("Export fingerprint is invalid")
        }
        val sink = activity.contentResolver.openOutputStream(uri, "wt")
          ?: throw IllegalStateException("Provider did not open an export write stream")
        val digest = MessageDigest.getInstance("SHA-256")
        var total = 0L
        staged.inputStream().use { input ->
          sink.use { output ->
            val buffer = ByteArray(64 * 1024)
            while (true) {
              val count = input.read(buffer)
              if (count < 0) break
              total += count
              if (total > maxBytes) throw IllegalArgumentException("Export exceeds output budget")
              digest.update(buffer, 0, count)
              output.write(buffer, 0, count)
            }
            output.flush()
          }
        }
        val actual = digestHex(digest.digest())
        if (total != args.bytes || actual != args.sha256) {
          throw IllegalStateException("Staging file changed during export")
        }
        val (confirmedBytes, confirmedHash) = digestDocument(uri, maxBytes)
        if (confirmedBytes != args.bytes || confirmedHash != args.sha256) {
          throw IllegalStateException("Provider returned different export content")
        }
        synchronized(pendingExports) { pendingExports.remove(args.uri) }
        invoke.resolve(JSObject().apply {
          put("uri", args.uri)
          put("bytes", confirmedBytes)
          put("sha256", confirmedHash)
        })
      } catch (error: Exception) {
        discardPendingExport(args.uri)
        invoke.reject(error.message ?: "Export commit failed")
      }
    }.start()
  }

  @Command
  fun abortExportDocument(invoke: Invoke) {
    val args = invoke.parseArgs(DocumentUriArgs::class.java)
    Thread {
      try {
        requireContentUri(args.uri)
        discardPendingExport(args.uri)
        invoke.resolve()
      } catch (error: Exception) {
        invoke.reject(error.message ?: "Export cleanup failed")
      }
    }.start()
  }

  private fun discardPendingExport(value: String) {
    if (!synchronized(pendingExports) { pendingExports.remove(value) }) return
    try { DocumentsContract.deleteDocument(activity.contentResolver, requireContentUri(value)) }
    catch (_: Exception) { /* Providers may not support deletion; failure is already reported. */ }
  }

  private fun digestDocument(uri: Uri, maxBytes: Long): Pair<Long, String> {
    val input = activity.contentResolver.openInputStream(uri)
      ?: throw IllegalStateException("Provider did not open an export read stream")
    return input.use { stream ->
      val digest = MessageDigest.getInstance("SHA-256")
      var total = 0L
      val buffer = ByteArray(64 * 1024)
      while (true) {
        val count = stream.read(buffer)
        if (count < 0) break
        total += count
        if (total > maxBytes) throw IllegalArgumentException("Export exceeds output budget")
        digest.update(buffer, 0, count)
      }
      total to digestHex(digest.digest())
    }
  }

  private fun digestHex(bytes: ByteArray): String = bytes.joinToString("") {
    (it.toInt() and 0xff).toString(16).padStart(2, '0')
  }

  @Command
  fun drainIntents(invoke: Invoke) {
    val uris = JSONArray()
    synchronized(pendingIntents) {
      while (pendingIntents.isNotEmpty()) uris.put(pendingIntents.removeFirst())
    }
    invoke.resolve(JSObject().apply { put("uris", uris) })
  }

  private fun captureIntent(intent: Intent?) {
    if (intent == null || intent === lastIntent) return
    lastIntent = intent
    val uri = when (intent.action) {
      Intent.ACTION_VIEW -> intent.data
      Intent.ACTION_SEND -> {
        @Suppress("DEPRECATION")
        intent.getParcelableExtra<Uri>(Intent.EXTRA_STREAM)
      }
      else -> null
    } ?: return
    if (uri.scheme != "content" || uri.authority.isNullOrBlank()) return
    synchronized(pendingIntents) {
      if (pendingIntents.size == MAX_PENDING_INTENTS) pendingIntents.removeFirst()
      pendingIntents.addLast(uri.toString())
    }
    trigger("openRequest", JSObject().apply { put("pending", true) })
  }

  private fun requireContentUri(value: String): Uri {
    val uri = Uri.parse(value)
    if (uri.scheme != "content" || uri.authority.isNullOrBlank()) {
      throw IllegalArgumentException("A content URI is required")
    }
    return uri
  }

  private fun validSegment(value: String): Boolean = value.isNotEmpty() && value != "." && value != ".." &&
    value.length <= 255 && value.none { it == '/' || it == '\\' || it.code < 32 || it.code == 127 }

  private fun pathParts(relative: String): List<String> {
    if (relative.isEmpty()) return emptyList()
    if (relative.length > 16384) throw IllegalArgumentException("Tree path exceeds limit")
    val parts = relative.split('/')
    if (parts.size > 64 || parts.any { !validSegment(it) }) throw IllegalArgumentException("Invalid tree path")
    return parts
  }

  private fun isMarkdown(name: String): Boolean = listOf(".md", ".markdown", ".txt")
    .any { name.endsWith(it, ignoreCase = true) }

  private fun isImage(name: String): Boolean = listOf(".png", ".jpg", ".jpeg", ".gif", ".webp")
    .any { name.endsWith(it, ignoreCase = true) }

  private fun directoryId(tree: Uri, relative: String): String {
    var parent = DocumentsContract.getTreeDocumentId(tree)
    for (segment in pathParts(relative)) {
      val matches = children(tree, parent).filter { it.name == segment && it.mime == DocumentsContract.Document.MIME_TYPE_DIR }
      if (matches.size != 1) throw IllegalArgumentException("Tree directory is unavailable or ambiguous")
      parent = matches[0].id
    }
    return parent
  }

  private fun children(tree: Uri, parentId: String): List<TreeChild> {
    val uri = DocumentsContract.buildChildDocumentsUriUsingTree(tree, parentId)
    val columns = arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID,
      DocumentsContract.Document.COLUMN_DISPLAY_NAME, DocumentsContract.Document.COLUMN_MIME_TYPE,
      DocumentsContract.Document.COLUMN_SIZE)
    val rows = ArrayList<TreeChild>()
    val cursor = activity.contentResolver.query(uri, columns, null, null, null)
      ?: throw IllegalStateException("Provider did not list tree children")
    cursor.use {
      var scanned = 0
      val idColumn = it.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
      val nameColumn = it.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
      val mimeColumn = it.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_MIME_TYPE)
      val sizeColumn = it.getColumnIndex(DocumentsContract.Document.COLUMN_SIZE)
      while (it.moveToNext()) {
        if (++scanned > MAX_TREE_CHILDREN) throw IllegalArgumentException("Tree directory exceeds 4096 entries")
        val id = it.getString(idColumn) ?: continue
        val name = it.getString(nameColumn) ?: continue
        val mime = it.getString(mimeColumn) ?: continue
        val size = if (sizeColumn >= 0 && !it.isNull(sizeColumn)) it.getLong(sizeColumn) else null
        rows.add(TreeChild(id, name, mime, size))
      }
    }
    return rows
  }

  private fun readBytes(uri: Uri): ByteArray {
    val stream = activity.contentResolver.openInputStream(uri)
      ?: throw IllegalStateException("Provider did not open a read stream")
    return stream.use { input ->
      val output = ByteArrayOutputStream()
      val buffer = ByteArray(64 * 1024)
      while (true) {
        val count = input.read(buffer)
        if (count < 0) break
        if (output.size() + count > MAX_DOCUMENT_BYTES) {
          throw IllegalArgumentException("Document exceeds 10 MiB")
        }
        output.write(buffer, 0, count)
      }
      output.toByteArray()
    }
  }

  private fun displayName(uri: Uri): String {
    return queriedName(uri) ?: uri.lastPathSegment?.take(255) ?: "Unknown"
  }

  private fun queriedName(uri: Uri): String? {
    activity.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
      if (cursor.moveToFirst()) {
        val column = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
        if (column >= 0) cursor.getString(column)?.let { return it.take(255) }
      }
    }
    return null
  }

  private fun digest(bytes: ByteArray): String = MessageDigest.getInstance("SHA-256")
    .digest(bytes).joinToString("") { (it.toInt() and 0xff).toString(16).padStart(2, '0') }
}
