define_source {
  name = "ewepkg",
  description = "Package manager",
  version = "0.1.0",
  architecture = "any",
  homepage = "https://github.com/hack3ric/ewepkg",
  license = "MIT",
  depends = { "rust", "openssl" },
  source = {
    -- todo
  }
}

define_package {
  package = function()
    -- todo
  end
}

define_package {
  name = "ewepkg-build",
  description = "Package builder",
  depends = { "lua", "sh", "fakeroot" },

  package = function()
    -- todo
  end
}
