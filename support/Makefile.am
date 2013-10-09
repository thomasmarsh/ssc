# 3rd Party Package Directories

ODE_DIR = ode-0.039
ODE_CLEAN = drawstuff contrib OPCODE tools include/drawstuff ode/test ode/fbuild ode/doc

EXTRA_DIST = $(ODE_DIR)

# Symbolic links (referenced in ssc src dir)

ode:
	ln -s $(ODE_DIR) ./ode

# Targets

all: ode
	(cd ode && make configure ode-lib)

clean: ode
	(cd ode && make clean)

distclean: ode
	(cd ode && make clean)

dist-hook: clean
	for dir in $(ODE_CLEAN); do $(RM) -rf $(ODE_DIR)/$$dir; done
