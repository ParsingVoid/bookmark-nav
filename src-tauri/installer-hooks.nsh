; Uninstall prompt: ask whether to keep the user's bookmark data.
; Data lives in the fixed app-data directory %APPDATA%\<identifier> (see
; fixed_bookmarks_path() in src-tauri/src/lib.rs). The identifier is hardcoded
; below rather than templated, since NSIS doesn't expose it as a variable here
; — keep this in sync with the "identifier" field in tauri.conf.json if it ever changes.
!macro NSIS_HOOK_PREUNINSTALL
  MessageBox MB_YESNO|MB_ICONQUESTION "是否保留你的书签数据？$\r$\n$\r$\n选择“是”将保留数据（重新安装后可继续使用）；选择“否”将一并删除。" IDYES bookmarknav_keep_data
    RMDir /r "$APPDATA\com.bookmark-nav.desktop"
  bookmarknav_keep_data:
!macroend
