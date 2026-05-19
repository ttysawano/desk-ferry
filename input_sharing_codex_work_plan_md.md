# TCP/IP 入力共有アプリ Codex向け実装作業手順書

## 1. この文書の目的

この文書は、要件定義書 `TCP/IP 入力共有アプリ ソフトウェア要件定義書 ver.2` をもとに、GitHub公開リポジトリ上で開発を進めるためのCodex向け作業指示である。

本プロジェクトは、Windows 10/11ホストに接続された物理マウス・キーボードを使い、同一LAN内のLinux X11クライアントを操作する入力共有アプリケーションを開発する。

最終サポート構成は以下に限定する。

```text
Server / Host:
  Windows 10/11
  複数ディスプレイ対応

Client:
  Linux on X11
  単一ディスプレイのみ対応
```

本作業手順では、最初から完成版を作ろうとせず、4段階に分けて実装・テスト・統合を進める。

## 2. リポジトリ方針

### 2.1 推奨方針

本プロジェクトは、WindowsサーバーとLinux X11クライアントを **同一GitHubリポジトリ内で管理する monorepo 構成** とする。

理由は以下である。

- サーバーとクライアントは同一プロトコルを共有する。
- 設定ファイルスキーマを共有する。
- セキュリティ仕様を共有する。
- テスト用モックサーバー・モッククライアントを共有できる。
- プロトコル変更時にサーバー・クライアント・テストを同一Pull Requestで更新できる。
- 初期開発段階では、分割リポジトリよりも仕様の齟齬を避けやすい。

### 2.2 分割リポジトリにしない理由

初期段階で以下のように分けないこと。

```text
windows-server repo
linux-client repo
protocol repo
```

分割すると、以下の問題が生じやすい。

- プロトコル変更時に複数リポジトリを同時に更新する必要がある。
- Codexに渡す作業範囲が分散する。
- テスト用のモックや共通仕様の同期が難しくなる。
- 初期PoC段階で管理コストが増える。

将来的に十分安定し、配布単位や保守体制が明確に分かれた場合のみ、リポジトリ分割を再検討する。

## 3. 推奨ディレクトリ構成

初期リポジトリ構成は以下とする。

```text
input-share/
  README.md
  LICENSE
  .gitignore
  docs/
    requirements-v2.md
    codex-work-plan.md
    protocol.md
    security.md
    config.md
    testing.md
  examples/
    server-windows.toml
    client-linux-x11.toml
    secret.psk.example
  schema/
    config.schema.json
    protocol.schema.json
  src/
    common/
      protocol/
      config/
      security/
      logging/
    server-windows/
      app/
      platform/
      tests/
    client-linux-x11/
      app/
      platform/
      tests/
    tools/
      mock-server/
      mock-client/
      protocol-dump/
  tests/
    integration/
    protocol/
    security/
  scripts/
    dev/
    test/
    package/
```

実装言語やビルドシステムにより、実際のディレクトリ名は調整してよい。

ただし、以下の分離は必ず維持する。

```text
共通仕様・共通処理
Windows依存処理
Linux X11依存処理
テスト用モック
ドキュメント
設定例
```

## 4. 実装上の基本方針

### 4.1 OS依存コードを隔離する

Windows API、X11 API、XTEST APIをアプリケーションロジックに直接混ぜない。

以下のような抽象境界を設ける。

```text
WindowsInputCaptureBackend
WindowsInputSuppressBackend
WindowsDisplayInfoBackend
WindowsCursorControlBackend

X11InputInjectBackend
X11CursorMonitorBackend
X11DisplayInfoBackend
```

アプリケーション本体は、抽象化されたインターフェースを通じてOS依存処理を呼び出す。

### 4.2 プロトコルを先に固定する

Windows入力取得やX11入力注入の実装に進む前に、まず以下を実装・テストする。

- メッセージ型
- JSON Linesシリアライズ
- JSON Linesデシリアライズ
- プロトコルバージョン確認
- Helloメッセージ
- mouse_move
- mouse_warp
- mouse_button
- key
- boundary_request
- active_host_changed
- release_all

### 4.3 セキュリティを後回しにしない

平文TCPだけでアプリケーション本体を作り込まない。

初期PoCでは一時的にローカルループバック上の平文モックを使ってもよいが、実機LANテストに進む前に必ず暗号化通信を入れる。

推奨する初期方式は以下である。

```text
TCP接続
  -> TLSで暗号化
  -> サーバー証明書フィンガープリントをクライアント側で確認
  -> TLS確立後、PSKチャレンジレスポンスでクライアントを認証
  -> 認証成功後に入力イベント通信を開始
```

### 4.4 まずモックで動かす

Windows APIやX11 APIに依存する前に、以下のモックを作る。

- mock-server
- mock-client
- protocol-dump

モック間で以下を確認する。

- 接続できること。
- Helloを交換できること。
- 認証に成功・失敗できること。
- 入力イベントを送受信できること。
- 境界遷移要求を処理できること。
- release_allを送受信できること。

## 5. 禁止事項

Codexは、以下を行ってはならない。

### 5.1 スコープに関する禁止事項

- Linuxホストを実装しない。
- Windowsクライアントを実装しない。
- macOS対応を実装しない。
- Wayland対応を実装しない。
- クライアント側複数ディスプレイ対応を実装しない。
- GUI設定ツールを実装しない。
- クリップボード共有を実装しない。
- ファイル転送を実装しない。
- 自動ホスト検出を実装しない。
- mDNS探索を実装しない。
- サービス化、常駐デーモン化を実装しない。

### 5.2 セキュリティに関する禁止事項

- 独自暗号を実装しない。
- 平文TCPを通常運用の既定値にしない。
- PSKをソースコードに埋め込まない。
- 秘密鍵やPSKをGit管理しない。
- キー入力内容をログに出力しない。
- 入力文字列をログに出力しない。
- 具体的なキーストローク列をログに出力しない。
- クリップボード内容を扱わない。
- ホスト名だけでクライアントを信用しない。

### 5.3 実装品質に関する禁止事項

- OS依存API呼び出しをアプリケーション本体へ直書きしない。
- サーバーとクライアントで別々のプロトコル定義を持たない。
- 設定ファイル例と実際の設定パーサーを乖離させない。
- テストなしでプロトコルを変更しない。
- 例外やエラーを握りつぶさない。
- 接続断時に押下中キーやマウスボタンを放置しない。
- 非常停止キーの処理を後回しにしない。

## 6. 制約条件

### 6.1 サポート制約

- サーバーはWindows 10/11のみ。
- サーバー側は複数ディスプレイ対応必須。
- クライアントはLinux X11のみ。
- クライアント側は単一ディスプレイのみ。
- X11以外のLinuxセッションでは動作保証しない。

### 6.2 入力制約

- キーボード入力は物理キーコード方式とする。
- サーバーとクライアントのキーボード配列は同一前提とする。
- IME状態同期はしない。
- クライアント側の物理キーボード・マウスとの競合は許容する。

### 6.3 遷移制約

- 初期PoCでは左右方向の遷移のみ実装する。
- 境界遷移条件は「画面端に到達 + さらに外向き移動」とする。
- 遷移時はエッジ上の相対位置を保存する。
- 遷移直後は `entry_margin_px` だけ内側にカーソルを置く。
- 遷移後は `cooldown_ms` の間、再遷移を抑制する。

### 6.4 運用制約

- 初期実装は手動起動のCUIアプリとする。
- インストーラは不要。
- Windowsサービス化はしない。
- Linux systemd service化はしない。
- 実機テストはローカルLAN内で行う。

## 7. 開発段階

## Stage 1: リポジトリスケルトンと共通仕様の作成

### 7.1 目的

最初の段階では、実OSの入力制御には入らず、プロジェクト構造、ドキュメント、設定ファイル、プロトコル型、モックテストの土台を作る。

### 7.2 実装タスク

- README.mdを作成する。
- LICENSEを追加する。
- `.gitignore` を追加する。
- `docs/requirements-v2.md` を追加する。
- `docs/codex-work-plan.md` を追加する。
- `docs/protocol.md` を作成する。
- `docs/security.md` を作成する。
- `docs/config.md` を作成する。
- `examples/server-windows.toml` を作成する。
- `examples/client-linux-x11.toml` を作成する。
- `examples/secret.psk.example` を作成する。
- `src/common/` を作成する。
- `src/server-windows/` を作成する。
- `src/client-linux-x11/` を作成する。
- `src/tools/mock-server/` を作成する。
- `src/tools/mock-client/` を作成する。

### 7.3 実装内容

以下の共通データ型を作る。

```text
ProtocolVersion
HostName
Edge
MouseMoveEvent
MouseWarpEvent
MouseButtonEvent
KeyEvent
BoundaryRequest
ActiveHostChanged
ReleaseAll
HelloMessage
```

以下の処理を作る。

```text
JSON Lines encode
JSON Lines decode
Protocol version check
Config load
Config validation
Safe logger
```

### 7.4 テスト

Stage 1では、実機入力を使わず、ユニットテストのみを行う。

必須テスト:

- 設定ファイルを読み込めること。
- 不正な設定ファイルを拒否できること。
- neighbor指定をパースできること。
- JSON Linesで各メッセージをencode/decodeできること。
- 不明なメッセージ種別を拒否できること。
- protocol_version不一致を検出できること。
- loggerがキー入力内容を出力しないこと。

### 7.5 完了条件

- CIまたはローカルテストで全ユニットテストが通る。
- READMEにプロジェクト概要とサポート対象が明記されている。
- 設定ファイル例が要件定義と整合している。
- プロトコルの最小メッセージが定義済みである。

## Stage 2: 暗号化通信・認証・モックサーバー/クライアント

### 8.1 目的

実OS入力を扱う前に、暗号化されたTCP通信、PSK認証、プロトコル送受信、状態遷移をモックで確認する。

### 8.2 実装タスク

- TLS通信層を実装する。
- サーバー証明書フィンガープリント確認を実装する。
- PSKチャレンジレスポンス認証を実装する。
- mock-serverを実装する。
- mock-clientを実装する。
- protocol-dumpツールを実装する。
- 接続状態管理を実装する。
- active_host管理を実装する。

### 8.3 PSKチャレンジレスポンス仕様

平文でPSKを送信してはならない。

例として、以下の流れを採用する。

```text
1. TLS接続確立
2. server -> client: auth_challenge { nonce }
3. client -> server: auth_response { host, hmac }
4. hmac = HMAC-SHA256(psk, nonce || host || protocol_version)
5. serverが同じHMACを計算して検証
6. 成功した場合のみ authenticated = true
```

### 8.4 テスト

必須テスト:

- 正しいPSKで認証成功すること。
- 誤ったPSKで認証失敗すること。
- 未許可ホスト名で認証失敗すること。
- TLSなしでは通常モードが起動しないこと。
- 認証前に入力イベントを受け付けないこと。
- 接続断時にactive_hostがサーバーへ戻ること。
- release_allメッセージを送信できること。
- キーコードやキーストローク列がログに出ないこと。

### 8.5 完了条件

- mock-serverとmock-clientがTLS上で接続できる。
- PSK認証が成功・失敗ともにテストできる。
- 入力イベントをJSON Linesで送受信できる。
- active_hostの切替がモック上で確認できる。
- ログに秘密情報やキー入力内容が出ない。

## Stage 3: Windowsサーバー実装

### 9.1 目的

Windowsホスト側で、複数ディスプレイ列挙、マウス境界検出、入力取得、入力抑止、クライアントへのイベント送信を実装する。

この段階では、Linux実機クライアントではなくmock-clientへの送信を先に確認してよい。

### 9.2 実装タスク

- Windowsディスプレイ列挙を実装する。
- Windows仮想スクリーン座標を取得する。
- 設定ファイルの `display_neighbors` と実ディスプレイを対応付ける。
- マウスカーソル位置の監視を実装する。
- 特定ディスプレイ端での境界判定を実装する。
- 「端に到達 + さらに外向き移動」の判定を実装する。
- Low Level Mouse Hookを実装する。
- Low Level Keyboard Hookを実装する。
- アクティブホストがクライアントの場合の入力送信を実装する。
- 可能な範囲でWindows側入力抑止を実装する。
- 非常停止キーを実装する。
- 接続断時のフェイルセーフを実装する。

### 9.3 注意点

Windowsの複数ディスプレイ座標では、XまたはYが負になる場合がある。

以下のような単純な仮定をしてはならない。

```text
left = 0
right = total_width
top = 0
bottom = total_height
```

必ず各ディスプレイの矩形とWindows仮想スクリーン座標を使って判定する。

### 9.4 テスト

#### 9.4.1 ユニットテスト

- ディスプレイ矩形から右端・左端・上端・下端を計算できること。
- 負座標を含むディスプレイ配置を扱えること。
- 画面端到達だけでは遷移せず、外向き移動で遷移すること。
- cooldown中は再遷移しないこと。
- entry_margin_pxが反映されること。
- pos_ratioが正しく計算されること。

#### 9.4.2 手動テスト

- Windows上でディスプレイ一覧をログ出力できること。
- 指定ディスプレイ右端からmock-clientへboundaryイベントを送信できること。
- アクティブホストをmock-clientに切り替えられること。
- キーボードイベントをmock-clientへ送信できること。
- マウスイベントをmock-clientへ送信できること。
- 非常停止キーでWindowsへ戻ること。

### 9.5 完了条件

- Windowsホスト単体で複数ディスプレイを認識できる。
- 設定したディスプレイ端で境界遷移を検出できる。
- mock-clientに入力イベントを送信できる。
- 非常停止キーが機能する。
- 接続断時にWindowsへ戻る。

## Stage 4: Linux X11クライアント実装と実機統合テスト

### 10.1 目的

Linux X11クライアントで、受信した入力イベントをX11へ注入し、実際にWindowsホストからLinuxクライアントを操作できる状態にする。

### 10.2 実装タスク

- X11セッション確認を実装する。
- 単一ディスプレイ確認を実装する。
- X11ルートウィンドウサイズ取得を実装する。
- XTEST入力注入を実装する。
- mouse_moveをX11へ注入する。
- mouse_warpをX11へ注入する。
- mouse_buttonをX11へ注入する。
- mouse wheelをX11へ注入する。
- key down/upをX11へ注入する。
- クライアント側カーソル位置監視を実装する。
- クライアント側境界遷移要求を実装する。
- 接続断時のrelease_allを実装する。
- 再接続処理を実装する。

### 10.3 X11側の注意点

- Waylandでは動作保証しない。
- XTEST拡張が利用可能か起動時に確認する。
- X11キーコードと内部物理キーコードの変換テーブルを明示的に持つ。
- キーボード配列変換は行わない。
- IME状態同期は行わない。
- クライアント側物理入力との競合は許容する。

### 10.4 テスト

#### 10.4.1 ユニットテスト

- 内部キーコードからX11キーコードへ変換できること。
- 未対応キーを明確にエラーまたは無視できること。
- mouse_moveイベントを注入用コマンドへ変換できること。
- mouse_buttonイベントを注入用コマンドへ変換できること。
- boundary_requestを生成できること。
- 接続断時にrelease_all相当の処理を呼べること。

#### 10.4.2 Linux単体手動テスト

- X11セッションで起動できること。
- Waylandセッションでは明確なエラーを出すこと。
- XTEST拡張がない場合に明確なエラーを出すこと。
- mock-serverからのmouse_moveでカーソルが動くこと。
- mock-serverからのkeyイベントで文字入力またはショートカット入力が発生すること。
- 左端への外向き移動でboundary_requestを送れること。

#### 10.4.3 Windows + Linux 実機統合テスト

- WindowsホストからLinuxクライアントへ接続できること。
- Windowsホストの指定ディスプレイ右端からLinuxクライアント左端へ遷移できること。
- Linuxクライアント左端からWindowsホスト指定ディスプレイ右端へ戻れること。
- Linux上でマウス移動できること。
- Linux上で左クリック・右クリックできること。
- Linux上でホイール操作できること。
- Linux上でキーボード入力できること。
- Ctrl、Shift、Altを含むキー操作が破綻しないこと。
- 非常停止キーでWindowsへ戻れること。
- Linuxクライアント停止時にWindowsへ戻れること。

### 10.5 完了条件

- Windowsホストの物理マウス・キーボードでLinux X11クライアントを操作できる。
- 左右方向の往復遷移ができる。
- クライアント切断時に安全にWindowsへ戻る。
- 非常停止キーが機能する。
- ログにキー入力内容が出ない。

## 8. テスト戦略

### 8.1 テストの階層

本プロジェクトでは、以下の順にテストする。

```text
1. pure unit tests
2. protocol tests
3. security/auth tests
4. mock server/client tests
5. Windows backend manual tests
6. Linux X11 backend manual tests
7. Windows + Linux integration tests
```

### 8.2 自動テストで扱うもの

自動テストでは、以下を重点的に扱う。

- 設定ファイルの読み込み
- 設定値検証
- プロトコルencode/decode
- セキュリティ認証ロジック
- 状態遷移
- 座標変換
- 境界判定
- cooldown処理
- entry_margin処理
- ログ安全性

### 8.3 手動テストで扱うもの

OS依存入力処理は、手動テストを併用する。

- Windows低レベルフック
- Windows入力抑止
- Windows複数ディスプレイ列挙
- X11入力注入
- X11カーソル監視
- 実機LANでの遅延
- 非常停止キー

### 8.4 テスト用ログの注意

テスト中であっても、以下をログに出力してはならない。

- 入力された文字列
- キーコード列
- 具体的なキーストローク列
- PSK
- 秘密鍵

イベント種別と統計情報は出力してよい。

例:

```text
OK: sent key event
NG: sent KEY_A down with modifiers=[shift]
```

後者は具体的なキーストロークを含むため、通常ログには出さない。

## 9. Codexへの作業依頼テンプレート

Codexに作業を依頼するときは、以下の形式を用いる。

```md
# Task

このリポジトリは、Windows 10/11 host + Linux X11 client 専用のTCP/IP入力共有アプリです。
要件は docs/requirements-v2.md、作業手順は docs/codex-work-plan.md に従ってください。

今回の作業範囲は Stage X のみです。

## Scope

- 実装するもの:
  - ...
- 実装しないもの:
  - ...

## Constraints

- 平文通信を通常経路にしない。
- キー入力内容をログに出さない。
- OS依存コードを共通ロジックに混ぜない。
- Windows host + Linux X11 client 以外を実装しない。

## Required tests

- ...

## Deliverables

- 変更ファイル一覧
- 実装概要
- 実行したテスト
- 残課題
```

## 10. Stage別Codex指示例

### 10.1 Stage 1 指示例

```md
# Task

Stage 1として、リポジトリスケルトンと共通仕様の土台を作成してください。

## Scope

実装するもの:
- README.md
- docs/protocol.md
- docs/security.md
- docs/config.md
- examples/server-windows.toml
- examples/client-linux-x11.toml
- 共通プロトコル型
- JSON Lines encode/decode
- 設定ファイル読み込み
- 設定検証
- 安全なloggerの土台
- ユニットテスト

実装しないもの:
- Windows API呼び出し
- X11 API呼び出し
- 実TCP通信
- TLS通信
- 入力注入
- 入力抑止

## Required tests

- 設定ファイル読み込みテスト
- neighborパーステスト
- protocol encode/decodeテスト
- protocol_version不一致テスト
- loggerがキー入力内容を出さないことのテスト
```

### 10.2 Stage 2 指示例

```md
# Task

Stage 2として、TLS通信、PSKチャレンジレスポンス認証、mock-server、mock-clientを実装してください。

## Scope

実装するもの:
- TLS通信層
- サーバー証明書フィンガープリント確認
- PSKチャレンジレスポンス
- mock-server
- mock-client
- active_host状態管理
- 接続断処理

実装しないもの:
- Windows低レベルフック
- X11入力注入
- GUI
- クリップボード共有

## Required tests

- 正しいPSKで認証成功
- 誤ったPSKで認証失敗
- 未許可ホスト名で認証失敗
- 認証前の入力イベント拒否
- 接続断時にactive_hostがserverへ戻る
- ログにPSKやキー入力内容が出ない
```

### 10.3 Stage 3 指示例

```md
# Task

Stage 3として、Windowsサーバー側の複数ディスプレイ列挙、境界検出、入力取得、入力抑止、mock-clientへの入力イベント送信を実装してください。

## Scope

実装するもの:
- Windowsディスプレイ列挙
- Windows仮想スクリーン座標取得
- display_neighborsとの対応付け
- Low Level Mouse Hook
- Low Level Keyboard Hook
- 境界遷移判定
- 入力イベント送信
- 非常停止キー
- 接続断時フェイルセーフ

実装しないもの:
- Linux X11入力注入
- Windowsクライアント
- Linuxホスト
- Wayland
- クリップボード共有

## Required tests

- ディスプレイ矩形計算テスト
- 負座標を含むディスプレイ配置テスト
- pos_ratio計算テスト
- cooldownテスト
- entry_marginテスト
- 手動テスト手順の追加
```

### 10.4 Stage 4 指示例

```md
# Task

Stage 4として、Linux X11クライアントの入力注入、カーソル監視、境界遷移要求、Windows + Linux実機統合テストを実装してください。

## Scope

実装するもの:
- X11セッション確認
- 単一ディスプレイ確認
- XTEST利用可能性確認
- mouse_move注入
- mouse_warp注入
- mouse_button注入
- wheel注入
- key down/up注入
- クライアント側カーソル監視
- boundary_request送信
- 接続断時release_all
- 再接続処理

実装しないもの:
- Wayland対応
- クライアント側複数ディスプレイ
- IME同期
- キーボード配列変換
- GUI

## Required tests

- 内部キーコードからX11キーコードへの変換テスト
- X11セッション判定テスト
- mock-serverからの入力注入手動テスト
- Windows + Linux実機統合テスト手順の追加
```

## 11. コミット方針

### 11.1 コミット単位

コミットは、小さく意味のある単位に分ける。

例:

```text
Add repository skeleton
Add protocol message definitions
Add TOML config loader
Add PSK challenge-response authentication
Add Windows display enumeration backend
Add X11 input injection backend
```

### 11.2 避けるコミット

以下のような巨大コミットを避ける。

```text
Implement everything
Initial commit with all features
Fix many things
```

### 11.3 Pull Request単位

1つのPull Requestは、原則として1つのStageまたは1つの機能単位に対応させる。

## 12. CI方針

### 12.1 初期CI

初期CIでは、OS非依存のテストを優先する。

- protocol tests
- config tests
- security/auth tests
- logger safety tests
- coordinate conversion tests

### 12.2 OS依存CI

Windows APIやX11入力注入は、CIで完全には検証しない。

ただし、可能であれば以下を行う。

- Windows上でビルドが通ること。
- Linux上でビルドが通ること。
- X11依存コードは、ヘッドレス環境ではビルド確認までに留める。

実際の入力注入・入力抑止は手動テストで確認する。

## 13. リリース前チェックリスト

PoCリリース前に以下を確認する。

- READMEにサポート対象が明記されている。
- Windows host + Linux X11 client 以外をサポートすると誤解されない。
- 設定ファイル例が最新である。
- 平文通信が既定で無効である。
- PSKや秘密鍵がリポジトリに含まれていない。
- ログにキー入力内容が出ない。
- 非常停止キーが動作する。
- クライアント切断時にWindowsへ戻る。
- Windows複数ディスプレイの指定エッジから遷移できる。
- Linux X11クライアントが単一ディスプレイで動作する。
- Waylandでは明確なエラーを出す。

## 14. まとめ

本プロジェクトは、Windows 10/11ホストとLinux X11クライアントに限定した入力共有アプリケーションとして開発する。

リポジトリはmonorepoとし、サーバー、クライアント、共通プロトコル、セキュリティ処理、モック、テスト、ドキュメントを同一リポジトリ内で管理する。

開発は以下の4段階で進める。

```text
Stage 1: リポジトリスケルトンと共通仕様
Stage 2: 暗号化通信・認証・モック通信
Stage 3: Windowsサーバー実装
Stage 4: Linux X11クライアント実装と実機統合
```

Codexには、各Stageごとに作業範囲、禁止事項、必要テスト、成果物を明示して依頼する。

特に、平文通信、キー入力ログ出力、OS依存コードの混在、サポート対象外OSへの拡張、クリップボード共有などのスコープ拡大を禁止する。

