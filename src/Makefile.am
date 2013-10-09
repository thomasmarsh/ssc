INCLUDES = -I../support/ode/include

LIBS = ../support/ode/lib/libode.a \
	-lGLU -lGL -lSDL_mixer \
	$(SDL_LIBS) $(FT2_LIBS) -lpng -lz

bin_PROGRAMS	= ssc

ssc_h = audio.h control.h flock.h hud.h model.h ship.h blackhole.h \
	draw.h game.h level.h object.h bogey.h explode.h global.h \
	lunatic.h screen.h starfield.h common.h fatso.h graph.h missile.h \
	shield.h coord.h camera.h config.h font.h menu.h geom.h asteroid.h \
	glpng.h neural.h smarty.h physics.h handler.h

ssc_cc = global.cc audio.cc graph.cc screen.cc main.cc game.cc level.cc \
	control.cc model.cc hud.cc starfield.cc object.cc flock.cc \
	missile.cc ship.cc bogey.cc fatso.cc lunatic.cc blackhole.cc \
	coord.cc camera.cc config.cc font.cc menu.cc geom.cc asteroid.cc \
	glpng.c damage.cc smarty.cc physics.cc handler.cc

ssc_SOURCES = $(ssc_h) $(ssc_cc)
