//! どのブローカーでも成り立つ確認をまとめたテスト用のクレート (issue #522)
//!
//! BrokerProxy を受け取って確かめる関数を集め、各ブローカーのテストから呼ぶ。
//!
//! - [`api`]: BrokerProxy の API を一通り呼ぶ確認。core・local・HTTP・IPC・qmp・Zenoh のすべてで走らせる。
//! - [`data`]: 画像・数の配列・Topic の push をフレームで送る確認。IPC・qmp・Zenoh で走らせる。
//! - [`remote`]: 別の System の qmp・Zenoh ブローカーに繋ぐテストの本体。qmp と Zenoh で同じものを走らせる。
//! - [`fixture`]: 確認の相手にする System の設定と、その中の Process・Container の名前。
//!
//! ブローカーに固有の確認 (HTTP の自分宛ての識別子、qmp の切断、Zenoh の共有メモリなど) は
//! 各ブローカーのテストに置く。置き場所の決まりは CONTRIBUTING.md の「テスト」節。

pub mod api;
pub mod data;
pub mod fixture;
pub mod remote;
pub mod support;
