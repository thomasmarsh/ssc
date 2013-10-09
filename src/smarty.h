// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_SMARTY_H
#define SSC_SMARTY_H

#include "explode.h"
#include "neural.h"

const unsigned int SMARTY_MAX_SPEED = 10;
const unsigned int SMARTY_RADIUS = 15;
const double SMARTY_MASS = 20.0;

class Smarty : public ScreenObject
{
private:
	Smarty(Smarty&);
	Explosion<10> mExplosion;
	static NeuralNetwork<4,6,2> brain; // shared brain! how cool is that?
	Coord3<double> mLastPosition, mLastForce, mLastShipPos;
public:
	Smarty();
	~Smarty();

	void draw();

	void rotate(double amt);
	void move(double dt);
	bool collision(ScreenObject &other);
	void accelerate(double dt);
};


#endif // SSC_SMARTY_H
