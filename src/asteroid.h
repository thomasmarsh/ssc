#ifndef SSC_ASTEROID_H
#define SSC_ASTEROID_H

#include "object.h"
#include "geom.h"

class Asteroid : public ScreenObject
{
public:
	enum Size { LARGE=20, MEDIUM=10, SMALL=5 };

	Asteroid(double);
	void setup(double, int, int);

	~Asteroid() {}

	void draw();
	void move(double dt) { rot += rot_amt * dt; ScreenObject::move(dt); }
	bool collision(ScreenObject &other) { return true; }

	void split(int seed, double strength);

	Coord3<double> midpoint(Coord3<double> &p0,
				Coord3<double> &p1,
				Coord3<double> &bc,
				int seed, double strength);

	int points() { return 10; }
private:
	Mesh mMesh;
	double mSize;
	Coord3<double> rot, rot_amt;
	bool mFirstDraw;
	int mDisplayList;
};

#endif // SSC_ASTEROID_H
