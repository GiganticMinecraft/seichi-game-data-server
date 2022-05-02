# seichi-game-data-server

整地鯖のゲームDBに対して直接読み書きする必要があるデータをgRPCで露出するAPIサーバー。

APIはgRPCにより提供されており、プロトコル定義は
[seichi-game-data-protocol](https://github.com/GiganticMinecraft/seichi-game-data-protocol)
にて管理されています。
