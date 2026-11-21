# Built by packaging/build-packages.sh from a staged tree (the binary is
# compiled before rpmbuild runs).
Name:           xbnc
Version:        %{xbnc_version}
Release:        1%{?dist}
Summary:        IRC bouncer with a web panel
License:        Apache-2.0
URL:            https://github.com/
BuildArch:      %{xbnc_arch}
Requires:       ca-certificates, openssl, systemd
AutoReqProv:    no

%description
xBNC keeps you connected to IRC networks, stores history for each of your
devices and is managed from a web panel. https://xbnc.org

%install
cp -a %{xbnc_root}/. %{buildroot}/

%files
/usr/bin/xbncd
/usr/sbin/xbnc-setup
/usr/lib/systemd/system/xbnc.service
/usr/lib/systemd/system/xbnc-update.service
/usr/lib/systemd/system/xbnc-update.path
/usr/share/xbnc
/usr/share/doc/xbnc

%post
systemctl daemon-reload >/dev/null 2>&1 || :
systemctl enable --now xbnc-update.path >/dev/null 2>&1 || :
if [ "$1" -eq 1 ] && [ ! -f /etc/xbnc/xbnc.toml ]; then
    /usr/sbin/xbnc-setup || echo "xBNC: configuration failed; run 'sudo xbnc-setup' to retry."
else
    systemctl try-restart xbnc >/dev/null 2>&1 || :
fi

%preun
if [ "$1" -eq 0 ]; then
    systemctl disable --now xbnc xbnc-update.path >/dev/null 2>&1 || :
fi

%postun
systemctl daemon-reload >/dev/null 2>&1 || :
