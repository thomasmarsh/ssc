SUBDIRS = support src data

pkgdata_DATA = ssc.conf

EXTRA_DIST = FAQ TODO ChangeLog ssc.spec $(pkgdata_DATA)

rpm: dist
	mkdir -p build
	PWD=`pwd` rpmbuild --define "_sourcedir $$PWD" --define "_builddir $$PWD/build" \
		--define "_rpmdir $$PWD/build" --define "_srcrpmdir $$PWD"  -bb ssc.spec
	mv `find build -name '*.rpm'` ./
	rm -Rf build

zdist: dist-gzip
	(cd support && make clean)
	$(RM) ssc-@VERSION@.tar.bz2
	gunzip ssc-@VERSION@.tar.gz
	bzip2 -9 ssc-@VERSION@.tar
