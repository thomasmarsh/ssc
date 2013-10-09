dnl Process this file with autoconf to produce a configure script.

AC_INIT(ssc, 0.9, thomas.marsh@seznam.cz)

AC_CONFIG_AUX_DIR(scripts)

AC_PREREQ(2.50)

AC_CONFIG_SRCDIR([configure.ac])
AM_INIT_AUTOMAKE(ssc, 0.9, [dist-bzip2])
AM_CONFIG_HEADER(config.h)

AC_PROG_INSTALL
AC_PROG_MAKE_SET
AC_PROG_CXX
AC_PROG_CPP
AC_LANG_C

AC_CHECK_FUNCS([atexit])
AC_CHECK_FUNCS([rint])

AC_C_CONST
AC_C_INLINE

AC_HEADER_STDBOOL
AC_TYPE_SIZE_T

dnl Check for SDL
AM_PATH_SDL(1.1.5, , AC_MSG_ERROR([*** SDL version $SDL_VERSION not found!]))
LIBS="$LIBS $SDL_LIBS"
CPPFLAGS="$CPPFLAGS $SDL_CFLAGS"

AC_CHECK_HEADER(SDL.h,  , echo "No SDL.h header... cannot do anything !";exit 12
 )


dnl Check for FreeType
AC_CHECK_FT2(9.0.0, AC_MSG_ERROR([*** FreeType version $FT2_VERSION not found!]))
LIBS="$LIBS $FT2_LIBS"
CPPFLAGS="$CPPFLAGS $FT2_CFLAGS"

dnl Check for SDL_mixer
AC_CHECK_LIB(SDL_mixer,Mix_PlayMusic , , echo "==> No SDL_mixer ! Check <http://
www.libsdl.org/projects/SDL_mixer>";soundsdl=no)
AC_CHECK_HEADER(SDL_mixer.h,   , echo "No sound !"; soundsdl=no )

dnl Check for other libraries
AC_CHECK_LIB(GL,glVertex3f)
AC_CHECK_LIB(GLU,gluPerspective)
AC_CHECK_LIB(X11,XDestroyWindow)
AC_CHECK_LIB(Xi,XFreeDeviceControl)
AC_CHECK_LIB(Xmu,XmuScanlineNot)
AC_CHECK_LIB(png,png_write_rows , , echo "Download <http://www.cdrom.com/pub/png
>";exit 12)

dnl Check for important headers
AC_CHECK_HEADER(GL/gl.h, , echo "No GL/gl.h header... cannot do anything !";exit
 12 )
AC_CHECK_HEADER(GL/glu.h, , echo "No GL/glu.h header... cannot do anything !";ex
it 12 )
AC_CHECK_HEADER(GL/glx.h, , echo "No GL/glx.h header... cannot do anything !";ex
it 12 )
AC_CHECK_HEADER(png.h, , echo "No png.h header... cannot do anything !";exit 12
)


dnl Try to autodetect cpu type
CPU_NAME="unknown"
CPU_TYPE="unknown"
if test -e "/proc/cpuinfo" ; then
        intel[[30]]="-march=i386"
        intel[[32]]="-march=i386"
        intel[[34]]="-march=i386"
        intel[[40]]="-march=i486"
        intel[[41]]="-march=i486"
        intel[[42]]="-march=i486"
        intel[[43]]="-march=i486"
        intel[[44]]="-march=i486"
        intel[[45]]="-march=i486"
        intel[[47]]="-march=i486"
        intel[[48]]="-march=i486"
        intel[[51]]="-march=pentium"
        intel[[52]]="-march=pentium"
        intel[[54]]="-march=pentium-mmx"
        intel[[61]]="-march=pentiumpro"
        intel[[63]]="-march=pentium2"
        intel[[65]]="-march=pentium2"
        intel[[66]]="-march=pentium2"
        intel[[67]]="-march=pentium3"
        intel[[68]]="-march=pentium3"
        intel[[610]]="-march=pentium3"
        intel[[611]]="-march=pentium3"
        intel[[150]]="-march=pentium4"
        intel[[151]]="-march=pentium4"
        intel[[152]]="-march=pentium4"
        amd[[50]]="-march=i586"
        amd[[51]]="-march=i586"
        amd[[52]]="-march=i586"
        amd[[53]]="-march=i586"
        amd[[56]]="-march=k6"
        amd[[57]]="-march=k6"
        amd[[58]]="-march=k6-2"
        amd[[59]]="-march=k6-3"
        amd[[61]]="-march=athlon"
        amd[[62]]="-march=athlon"
        amd[[63]]="-march=athlon"
        amd[[64]]="-march=athlon"
        amd[[66]]="-march=athlon-xp"
        amd[[67]]="-march=athlon"
        amd[[68]]="-march=athlon"
        CPU_TYPE="known"
        CPU_FAMILY=`cat /proc/cpuinfo | grep "cpu family"`
        CPU_MODEL=`cat /proc/cpuinfo | grep model[[^\ ]]`
        CPU_NAME=`cat /proc/cpuinfo | grep "model name"`
        CPU_FLAGS=`cat /proc/cpuinfo | grep "flags"`
        CPU_VENDOR=`cat /proc/cpuinfo | grep "vendor_id"`
        CPU_FAMILY=${CPU_FAMILY#'cpu family        : '}
        CPU_MODEL=${CPU_MODEL#'model                : '}
        CPU_NAME=${CPU_NAME#'model name        : '}
        CPU_FLAGS=${CPU_FLAGS#'flags                : '}
        CPU_VENDOR=${CPU_VENDOR#'vendor_id        : '}
        if test "x${CPU_VENDOR}" = "xGenuineIntel" ; then
                CPU_OPTIONS=${intel[[$CPU_FAMILY$CPU_MODEL]]}
        fi
        if test "x${CPU_VENDOR}" = "xAuthenticAMD" ; then
                CPU_OPTIONS=${amd[[$CPU_FAMILY$CPU_MODEL]]}
        fi
        if test "x${CPU_OPTIONS}" = "x" ; then
                CPU_TYPE="unknown"
        fi
        CPU_EXT=""
        for i in $CPU_FLAGS ; do
                case $i in
                fpu)
                        CPU_FPU="-mfpmath=387"
                        ;;
                mmx)
                        CPU_EXT="$CPU_EXT -mmmx"
                        ;;
                sse)
                        CPU_FPU="-mfpmath=sse -msse"
                        ;;
                sse2)
                        CPU_FPU="-mfpmath=sse -msse2"
                        ;;
                3dnow)
                        CPU_EXT="$CPU_EXT -m3dnow"
                        ;;
                esac
        done
        CPU_OPTIONS="$CPU_OPTIONS $CPU_FPU $CPU_EXT"
fi
if test "x${CPU_TYPE}" = "xunknown"; then
        CPU_TYPE=`( uname -p ) 2>&1`
        case $CPU_TYPE in
        i386)
                CPU_OPTIONS="-march=i386"
                ;;
        i486)
                CPU_OPTIONS="-march=i486"
                ;;
	athlon)
		CPU_OPTIONS="-march=athlon-xp"
		;;
        Pentium2)
                CPU_OPTIONS="-march=pentium2"
                ;;
        Pentiumpro)
                CPU_OPTIONS="-march=pentiumpro"
                ;;
        Pentium*)
                CPU_OPTIONS="-march=pentium"
                ;;
        k6)
                CPU_OPTIONS="-march=k6"
                ;;
        k6-2)
                CPU_OPTIONS="-march=k6-2"
                ;;
        k6-3)
                CPU_OPTIONS="-march=k6-3"
                ;;
        *)
                CPU_OPTIONS=""
                CPU_TYPE="unknown"
                ;;
        esac
        if test "x${CPU_TYPE}" = "xunknown"; then
                CPU_TYPE=`( uname -m ) 2>&1`
                case $CPU_TYPE in
                i386)
                        CPU_OPTIONS="-march=i386"
                        ;;
                i486)
                        CPU_OPTIONS="-march=i486"
                        ;;
                i586)
                        CPU_OPTIONS="-march=i586"
                        ;;
                i686)
                        CPU_OPTIONS="-march=i686"
                        ;;
                Pentium2)
                        CPU_OPTIONS="-march=pentium2"
                        ;;
                Pentiumpro)
                        CPU_OPTIONS="-march=pentiumpro"
                        ;;
                k6)
                        CPU_OPTIONS="-march=k6"
                        ;;
                k6-2)
                        CPU_OPTIONS="-march=k6-2"
                        ;;
                k6-3)
                        CPU_OPTIONS="-march=k6-3"
                        ;;
                *)
                        CPU_OPTIONS=""
                        ;;
                esac
        fi
fi
echo "Detected CPU:`echo $CPU_NAME | sed -e 's/.*://'`"
dnl Now we check if the compiler supports the detected cpu
COMPILER=$CXX
for I in "$TMPDIR" "$TEMPDIR" "/tmp" ; do
        test "$I" && break
done
TMPC="$I/cpu_test-$RANDOM-$$.c"
TMPO="$I/cpu_test-$RANDOM-$$.o"
cat > $TMPC << EOF
int main(void) { return 0; }
EOF
( $COMPILER $CPU_OPTIONS -o $TMPO $TMPC ) 2>&1
TMP="$?"
rm -f $TMPO
rm -f $TMPC
if test "x${TMP}" = "x1" ; then
        CPU_OPTIONS=""
        echo "No CPU optimizations will be added"
else
        echo "CPU optimization: $CPU_OPTIONS"
fi

dnl XXX: TODO: test each of the optimization flags separately:
dnl
dnl		-O3/-O2/-O
dnl		-pipe
dnl		-finline-functions
dnl		-falign-functions=4 (how to find out best alignment?)
dnl		-mfancy-math-387 (how to find out?)
dnl		-fomit-frame-pointer

DBG_CXXFLAGS="$UNAME_DEFS  -g -Wall -Werror"
OPT_CXXFLAGS="$UNAME_DEFS -DNDEBUG -O3 -pipe -finline-functions -falign-functions=4 -ffast-math -fno-rtti -fno-exceptions -mfancy-math-387 -fomit-frame-pointer  $CPU_OPTIONS  -Wall -Werror"
PRF_CXXFLAGS="$UNAME_DEFS  -O3 -pipe -finline-functions -falign-functions=4 -mfancy-math-387 -pg $CPU_OPTIONS  -Wall -Werror"

CXXFLAGS=$OPT_CXXFLAGS

dnl Expand DATADIR
dnl FC_EXPAND_DIR(DATADIR, "$datadir/games/glaxium")
dnl AC_DEFINE_UNQUOTED(DATADIR, "$DATADIR")


AC_CONFIG_FILES([
Makefile
support/Makefile
src/Makefile
data/Makefile
ssc.spec
])
AC_OUTPUT

echo
echo "Compilation flags: $CXXFLAGS"
