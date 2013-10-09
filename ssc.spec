Name:		ssc
Version:	0.9
Release:	1

Group:		Amusements/Games
Summary:	A free OpenGL arcade space shoot-em-up with interesting physics

Vendor:		Thomas Marsh <thomas.marsh@seznam.cz>
Packager:	Thomas Marsh <thomas.marsh@seznam.cz>
License:	GPL
URL:		http://sscx.sourceforge.net/
Source0:	%{name}-%{version}.tar.gz
BuildRoot:	%{_tmppath}/%{name}-%{version}-root
Prefix:		/usr

%description
SSC is a 2D space shoot-em-up featuring interesting physics and alife. Destroy all enemies on screen to proceed to the next level. Inherits a lot of ideas from the game Koules.

%prep
%setup

%build
./configure --prefix=/usr
make

%install
make DESTDIR=%{buildroot} install


%clean
rm -rf %{buildroot}

%files
%defattr(-,root,root)
%doc AUTHORS COPYING INSTALL NEWS README ChangeLog TODO FAQ
%{_bindir}/*
%{_prefix}/share/ssc/*
