// --------------------------------------------------------------------------
//
// Copyright (c) 2003 Thomas D. Marsh. All rights reserved.
//
// "SSC" is free software; you can redistribute it
// and/or use it and/or modify it under the terms of
// the "GNU General Public License" (GPL).
//
// --------------------------------------------------------------------------

#ifndef SSC_SHIP_H
#define SSC_SHIP_H

#include "shield.h"
#include "explode.h"

enum PowerUpType
{
	POWERUP_NONE,
	POWERUP_INVINCIBILITY,
	POWERUP_FASTFIRE,
	POWERUP_FIREBALL,
	POWERUP_MEGABLAST,
	POWERUP_LAST
};

enum PowerUpLimit
{
	POWERUP_LIMIT_NONE,
	POWERUP_LIMIT_TIME,
	POWERUP_LIMIT_COUNT
};

inline PowerUpLimit getPowerUpLimit(PowerUpType t)
{
	switch (t)
	{
	case POWERUP_NONE:
	case POWERUP_LAST:
		return POWERUP_LIMIT_NONE;
	case POWERUP_INVINCIBILITY:
	case POWERUP_FASTFIRE:
		return POWERUP_LIMIT_TIME;
	case POWERUP_FIREBALL:
	case POWERUP_MEGABLAST:
		return POWERUP_LIMIT_COUNT;
	}
}

inline double getPowerUpLifetime(PowerUpType t)
{
	switch (t)
	{
	case POWERUP_NONE:
		return 0;
	case POWERUP_INVINCIBILITY:
		return 150;
	case POWERUP_FASTFIRE:
		return 300;
	case POWERUP_FIREBALL:
		return 3;
	case POWERUP_MEGABLAST:
		return 3;
	case POWERUP_LAST:
		return 0;
	}
}


//	PowerUpType mPowerUp;
//	int mPUCount;
//	double mPUAge;

class Ship : public ScreenObject
{
public:
	enum MissileLevel
	{
		MISSILE_NORMAL,
		MISSILE_FAST,
		MISSILE_TRIPLE,
		MISSILE_QUINTUPLE,
		MISSILE_FULL
	};

	Ship(double x, double y);
	~Ship();

	void draw();
	void fire();
	void move(double dt);
	void accelerate(double amt);
	void init();

	bool collision(ScreenObject &obj);

	void rotate(double amt);
	void damage(double amt);
	void upgrade();

	double rx, ry;
	Shield shield;
	Explosion<100> explosion;
	double mLife;
	unsigned int mBombCount;
	bool mCanFire;
	double mFireCounter, mRateOfFire;
	MissileLevel mMissileLevel;
};

#endif // SSC_SHIP_H
