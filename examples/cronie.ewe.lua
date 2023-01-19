local version = "1.6.1"
local revision = "1"

define_source {
  name = "cronie",
  description = "Daemon that runs specified programs at scheduled times and related tools",
  version = version .. "-" .. revision,
  architecture = "any",
  homepage = "https://github.com/cronie-crond/cronie/",
  license = { "apache", "custom:CC0" },
  depends = { "pam", "bash", "run-parts" },
  optional_depends = {
    { name = "smtp-server", description = "send job output via email" },
    { name = "smtp-forwarder", description = "forward job output to email server" }
  },
  source = {
    { url = ("https://github.com/cronie.crond/cronie/releases/download/cronie-%s/cronie-%s.tar.gz")
        :format(version, version),
      sha256sum = "2cd0f0dd1680e6b9c39bf1e3a5e7ad6df76aa940de1ee90a453633aa59984e62" },
    { path = "80-cronie.hook",
      sha256sum = "f85e9a68bf3bf446f8a6167f068371c06afffe11ca71935d8ee5487b38b2c9db" },
    { path = "service",
      sha256sum = "ac3ff3c8a5ce1b6367b06877b4b12ff74e7f18a3c510fb9f80d6ea6b6321e3b1" },
    { path = "pam.d",
      sha256sum = "00864268b491bab8c66400a4a4b4bf85f168a6e44e85676105e084940924090c" },
    { path = "deny",
      sha256sum = "ae6e533ecdfc1bd2dd80a9e25acb0260cbe9f00c4e4abee93d552b3660f263fc" }
  },

  build = function()
    -- ewe.helpers.make()
    -- todo
  end,

  check = function()
    -- todo()
  end
}

-- Source only produces one package, name and description can be omitted.
define_package {
  package = function()
    -- todo
  end
}
