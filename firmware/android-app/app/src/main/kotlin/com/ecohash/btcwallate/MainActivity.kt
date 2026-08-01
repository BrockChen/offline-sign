package com.ecohash.btcwallate

import android.content.Intent
import android.os.Bundle

/** 路由：按有无钱包/是否已解锁决定首屏（对标 iOS makeRoot）。 */
class MainActivity : BaseActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Session.loadNet(this)
        val next = when {
            KC.load(this) == null -> SetupActivity::class.java
            Session.ks != null -> HomeActivity::class.java
            else -> UnlockActivity::class.java
        }
        startActivity(Intent(this, next))
        finish()
    }
}
